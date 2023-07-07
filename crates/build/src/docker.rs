// Copyright 2018-2023 Parity Technologies (UK) Ltd.
// This file is part of cargo-contract.
//
// cargo-contract is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// cargo-contract is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with cargo-contract.  If not, see <http://www.gnu.org/licenses/>.

//! This module provides a simple interface to execute the verifiable build
//! inside the docker container.
//!
//! For the correct behaviour, the docker engine must be running,
//! and the socket to be accessible.
//!
//! It is also important that the docker registry contains the tag
//! that matches the current version of this crate.
//!
//! The process of the build is following:
//! 1. Pull the image from the registry or use the local copy if available
//! 2. Parse other arguments that were passed to the host execution context
//! 3. Calculate the digest of the command and use it
//! to uniquely identify the container
//! 4. If the container exists, we just start the build, if not, we create it
//! 5. After the build, the docker container produces metadata with
//! paths relative to its internal storage structure, we parse the file
//! and overwrite those paths relative to the host machine.

use std::{
    collections::{
        hash_map::DefaultHasher,
        HashMap,
    },
    hash::{
        Hash,
        Hasher,
    },
    io::{
        BufReader,
        Write,
    },
    path::{
        Path,
    },
    time::Duration,
};

use anyhow::{
    Context,
    Result,
};
use bollard::{
    container::{
        AttachContainerOptions,
        LogOutput,
        Config,
        CreateContainerOptions,
        ListContainersOptions,
    },
    image::{
        CreateImageOptions,
        ListImagesOptions,
    },
    service::{
        HostConfig,
        ImageSummary,
        Mount,
        MountTypeEnum,
    },
    Docker,
};
use contract_metadata::ContractMetadata;
use indicatif::{
    ProgressBar,
    ProgressStyle,
};
use tokio_stream::StreamExt;

use crate::{
    maybe_println,
    BuildArtifacts,
    BuildResult,
    BuildSteps,
    CrateMetadata,
    ExecuteArgs,
    Verbosity,
    MetadataArtifacts,
};

use colored::Colorize;

const IMAGE: &str = "paritytech/contracts-verifiable";
// We assume the docker image contains the same tag as the current version of the crate
const VERSION: &str = env!("CARGO_PKG_VERSION");

const MOUNT_DIR: &str = "/contract";

#[derive(Clone, Debug, Default)]
pub enum ImageVariant {
    #[default]
    Default,
    Custom(String),
}

/// Launches the docker container to execute verifiable build
pub fn docker_build(args: ExecuteArgs) -> Result<BuildResult> {
    let ExecuteArgs {
        manifest_path,
        verbosity,
        output_type,
        target,
        build_artifact,
        image,
        ..
    } = args;
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let mut build_steps = BuildSteps::new();

            build_steps.set_total_steps(3);
            if build_artifact == BuildArtifacts::CodeOnly {
                build_steps.set_total_steps(2);
            }

            let crate_metadata = CrateMetadata::collect(&manifest_path, target)?;
            let host_dir = std::env::current_dir()?;
            let args = compose_build_args()?;

            let client = Docker::connect_with_socket_defaults().map_err(|e| {
                anyhow::anyhow!("{}\nDo you have the docker engine installed in path?", e)
            })?;
            let _ = client.ping().await.map_err(|e| {
                anyhow::anyhow!("{}\nIs your docker engine up and running?", e)
            })?;
            let build_image =
                get_image(client.clone(), image, &verbosity, &mut build_steps).await?;

            let build_result = run_build(
                args,
                &build_image,
                &crate_metadata.contract_artifact_name,
                &host_dir,
                &verbosity,
                &mut build_steps,
            )
            .await?;

            let metadata_artifacts = MetadataArtifacts {
                dest_metadata: crate_metadata.metadata_path(),
                dest_bundle: crate_metadata.contract_bundle_path(),
            };

            update_metadata(&metadata_artifacts, &verbosity, &mut build_steps, &build_image)?;

            Ok(BuildResult {
                output_type,
                verbosity,
                ..build_result
            })
        })
}

/// Overwrites `build_result` and `image` fields in the metadata
fn update_metadata(
    metadata_artifacts: &MetadataArtifacts,
    verbosity: &Verbosity,
    build_steps: &mut BuildSteps,
    build_image: &ImageSummary,
) -> Result<()> {
    let mut metadata = ContractMetadata::load(&metadata_artifacts.dest_bundle)?;
    // find alternative unique identifier of the image, otherwise grab the digest
    let image_tag = match build_image
        .repo_tags
        .iter()
        .find(|t| !t.ends_with("latest"))
    {
        Some(tag) => tag.to_owned(),
        None => build_image.id.clone(),
    };

    metadata.image = Some(image_tag);

    crate::metadata::write_metadata(
        &metadata_artifacts,
        metadata,
        build_steps,
        verbosity,
    )
}

/// Creates the container and executed the build inside it
async fn run_build(
    build_args: String,
    build_image: &ImageSummary,
    contract_name: &str,
    host_folder: &Path,
    verbosity: &Verbosity,
    build_steps: &mut BuildSteps,
) -> Result<BuildResult> {
    let client = Docker::connect_with_socket_defaults()?;

    let mut entrypoint = vec!["/bin/bash".to_string(), "-c".to_string()];
    let mut cmds = vec![format!(
        "mkdir -p target && cargo contract build {} --output-json",
        build_args,
    )];

    entrypoint.append(&mut cmds);

    let digest_code = container_digest(entrypoint.clone(), build_image.id.clone());
    let container_name =
        format!("ink-verified-{}-{}", contract_name, digest_code.clone());

    let mut filters = HashMap::new();
    filters.insert("name".to_string(), vec![container_name.clone()]);

    let containers = client
        .list_containers(Some(ListContainersOptions::<String> {
            all: true,
            filters,
            ..Default::default()
        }))
        .await?;

    let container_option = containers.first();

    let mount = Mount {
        target: Some(String::from(MOUNT_DIR)),
        source: Some(
            host_folder
                .to_str()
                .context("Cannot convert path to string.")?
                .to_string(),
        ),
        typ: Some(MountTypeEnum::BIND),
        ..Default::default()
    };
    let host_cfg = Some(HostConfig {
        mounts: Some(vec![mount]),
        ..Default::default()
    });

    let user;
    #[cfg(unix)]
    {
        user = Some(format!(
            "{}:{}",
            users::get_current_uid(),
            users::get_current_gid()
        ));
    };
    #[cfg(windows)]
    {
        user = None;
    }

    let mut labels = HashMap::new();
    labels.insert("digest-code".to_string(), digest_code);
    let config = Config {
        image: Some(build_image.id.clone()),
        entrypoint: Some(entrypoint),
        cmd: None,
        labels: Some(labels),
        host_config: host_cfg,
        attach_stderr: Some(true),
        tty: Some(true),
        user,
        ..Default::default()
    };
    let options = Some(CreateContainerOptions {
        name: container_name.as_str(),
        platform: Some("linux/amd64"),
    });

    let container_id = match container_option {
        Some(container) => {
            container
                .id
                .clone()
                .context("Container does not have an ID")?
        }
        None => client.create_container(options, config).await?.id,
    };

    client
        .start_container::<String>(&container_id, None)
        .await?;

    maybe_println!(
        verbosity,
        " {} {}",
        format!("{build_steps}").bold(),
        "Started the build inside the container"
            .bright_green()
            .bold(),
    );

    let mut attach_result = client
        .attach_container(
            &container_id,
            Some(AttachContainerOptions::<String> {
                stdout: Some(true),
                stderr: Some(true),
                stream: Some(true),
                ..Default::default()
            }),
        )
        .await?;

    // pipe docker attach output into stdout
    let stderr = std::io::stderr();
    let mut stderr = stderr.lock();
    while let Some(Ok(output)) = attach_result.output.next().await {
        // todo: handle build error here
        match output {
            LogOutput::StdOut { message } => {
                return serde_json::from_reader(BufReader::new(message.as_ref()))
                    .context("Failed to deserialize build result from docker build")
                // // todo: attempt to decode bytes to BuildResult and return Ok()
                // match serde_json::from_reader(BufReader::new(message)) {
                //     Ok(build_result) => {
                //         return Ok(build_result);
                //     }
                //     Err(_) => {
                //         stderr.write_all(message.as_ref())?;
                //     }
                // }
            }
            LogOutput::StdErr { message } => {
                stderr.write_all(message.as_ref())?;
            }
            LogOutput::Console { message } => todo!("LogOutput::Console: {:?}", message),
            LogOutput::StdIn { message: _ } => panic!("LogOutput::StdIn")
        }
        // stdout.write_all(output.into_bytes().as_ref())?;
        stderr.flush()?;
    }

    Err(anyhow::anyhow!("Failed to read build result from docker build"))
}

/// Takes CLI args from the host and appends them to the build command inside the docker
fn compose_build_args() -> Result<String> {
    use regex::Regex;
    let mut args: Vec<String> = vec!["--release".to_string()];

    let rex = Regex::new(r"--image [.*]*")?;
    let args_string: String = std::env::args().collect();
    let args_string = rex.replace_all(&args_string, "").to_string();

    let mut os_args: Vec<String> = args_string
        .split_ascii_whitespace()
        .filter(|a| {
            a != &"--verifiable"
                && !a.contains("cargo-contract")
                && a != &"cargo"
                && a != &"contract"
                && a != &"build"
        })
        .map(|s| s.to_string())
        .collect();

    args.append(&mut os_args);

    let joined_args = args.join(" ");
    Ok(joined_args)
}

/// Retrieve local of the image, otherwise pulls one from the registry
async fn get_image(
    client: Docker,
    custom_image: ImageVariant,
    verbosity: &Verbosity,
    build_steps: &mut BuildSteps,
) -> Result<ImageSummary> {
    // if no custom image is specified, then we use the tag of the current version of
    // `cargo-contract`
    let image = match custom_image {
        ImageVariant::Custom(i) => i.clone(),
        ImageVariant::Default => {
            format!("{}:{}", IMAGE, VERSION)
        }
    };

    let build_image = match find_local_image(client.clone(), image.clone()).await? {
        Some(image_s) => image_s,
        None => {
            build_steps.total_steps = build_steps.total_steps.map(|s| s + 1);
            pull_image(client.clone(), image.clone(), verbosity, build_steps).await?;
            find_local_image(client.clone(), image.clone())
                .await?
                .context("Could not pull the image from the registry")?
        }
    };

    Ok(build_image)
}

/// Searches for the local copy of the docker image
async fn find_local_image(client: Docker, image: String) -> Result<Option<ImageSummary>> {
    let images = client
        .list_images(Some(ListImagesOptions::<String> {
            all: true,
            ..Default::default()
        }))
        .await?;
    let build_image = images.iter().find(|i| i.repo_tags.contains(&image));

    Ok(build_image.cloned())
}

/// Pulls the docker image from the registry
async fn pull_image(
    client: Docker,
    image: String,
    verbosity: &Verbosity,
    build_steps: &mut BuildSteps,
) -> Result<()> {
    let mut pull_image_stream = client.create_image(
        Some(CreateImageOptions {
            from_image: image,
            ..Default::default()
        }),
        None,
        None,
    );

    maybe_println!(
        verbosity,
        " {} {}",
        format!("{build_steps}").bold(),
        "Image does not exist. Pulling one from the registry"
            .bright_green()
            .bold()
    );
    build_steps.increment_current();

    if verbosity.is_verbose() {
        let spinner_style = ProgressStyle::with_template(
            " {spinner:.cyan} [{wide_bar:.cyan/blue}]\n {wide_msg}",
        )?
        .progress_chars("#>-")
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ");
        let pb = ProgressBar::new(1000);
        pb.set_style(spinner_style);
        pb.enable_steady_tick(Duration::from_millis(100));

        while let Some(summary_result) = pull_image_stream.next().await {
            let summary = summary_result?;

            if let Some(progress_detail) = summary.progress_detail {
                let total = progress_detail.total.map_or(1000, |v| v) as u64;
                let current_step = progress_detail.current.map_or(1000, |v| v) as u64;
                pb.set_length(total);
                pb.set_position(current_step);

                if let Some(msg) = summary.status {
                    pb.set_message(msg);
                }
            }
        }

        pb.finish();
    } else {
        while pull_image_stream.next().await.is_some() {}
    }

    Ok(())
}

/// Calculates the unique container's code
fn container_digest(entrypoint: Vec<String>, image_digest: String) -> String {
    // in order to optimise the container usage
    // we are hashing the inputted command
    // in order to reuse the container for the same permutation of arguments
    let mut s = DefaultHasher::new();
    // the data is set of commands and args and the image digest
    let data = (entrypoint, image_digest);
    data.hash(&mut s);
    let digest = s.finish();
    // taking the first 5 digits to be a unique identifier
    let digest_code: String = digest.to_string().chars().take(5).collect();
    digest_code
}
