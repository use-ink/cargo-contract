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
    io::{
        BufReader,
        Write,
    },
    path::Path,
};

use anyhow::{
    Context,
    Result,
};
use bollard::{
    container::{
        AttachContainerOptions,
        AttachContainerResults,
        Config,
        CreateContainerOptions,
        LogOutput,
    },
    image::CreateImageOptions,
    service::{
        HostConfig,
        Mount,
        MountTypeEnum,
    },
    Docker,
};
use contract_metadata::ContractMetadata;
use tokio_stream::StreamExt;

use crate::{
    maybe_println,
    BuildArtifacts,
    BuildResult,
    BuildSteps,
    CrateMetadata,
    ExecuteArgs,
    MetadataArtifacts,
    Verbosity,
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

            let image = match image {
                ImageVariant::Custom(i) => i.clone(),
                ImageVariant::Default => {
                    format!("{}:{}", IMAGE, VERSION)
                }
            };

            if let Err(err) =
                pull_image(client.clone(), &image, &verbosity, &mut build_steps).await
            {
                // If the image could not be pulled, we will still attempt to use a local
                // image of that name if it exists.
                eprintln!(
                    "{}",
                    format!("Failed to pull the docker image {}: {}", image, err)
                        .yellow()
                        .bold(),
                );
            }

            let build_result = run_build(
                args,
                &image,
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

            update_metadata(&metadata_artifacts, &verbosity, &mut build_steps, &image)?;

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
    build_image: &str,
) -> Result<()> {
    let mut metadata = ContractMetadata::load(&metadata_artifacts.dest_bundle)?;

    metadata.image = Some(build_image.to_owned());

    crate::metadata::write_metadata(&metadata_artifacts, metadata, build_steps, verbosity)
}

/// Creates the container and executed the build inside it
async fn run_build(
    build_args: String,
    build_image: &str,
    contract_name: &str,
    host_folder: &Path,
    verbosity: &Verbosity,
    build_steps: &mut BuildSteps,
) -> Result<BuildResult> {
    let client = Docker::connect_with_socket_defaults()?;

    let entrypoint = vec!["cargo".to_string(), "contract".to_string()];

    let cmd = vec![
        "build".to_string(),
        "--release".to_string(),
        "--output-json".to_string(),
    ];

    let container_name = format!("ink-verified-{}", contract_name);

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

    let config = Config {
        image: Some(build_image.to_string()),
        entrypoint: Some(entrypoint),
        cmd: Some(cmd),
        // labels: Some(labels),
        host_config: host_cfg,
        attach_stderr: Some(true),
        // tty: Some(true),
        user,
        ..Default::default()
    };
    let options = Some(CreateContainerOptions {
        name: container_name.as_str(),
        platform: Some("linux/amd64"),
    });

    let container_id = client.create_container(options, config).await?.id;

    client
        .start_container::<String>(&container_id, None)
        .await?;

    let AttachContainerResults { mut output, .. } = client
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

    maybe_println!(
        verbosity,
        " {} {}",
        format!("{build_steps}").bold(),
        "Started the build inside the container"
            .bright_green()
            .bold(),
    );

    // pipe docker attach output into stdout
    let stderr = std::io::stderr();
    let mut stderr = stderr.lock();
    let mut build_result = None;
    while let Some(Ok(output)) = output.next().await {
        match output {
            LogOutput::StdOut { message } => {
                build_result = Some(
                    serde_json::from_reader(BufReader::new(message.as_ref()))
                        .context("Error decoding BuildResult"),
                );
            }
            LogOutput::StdErr { message } => {
                stderr.write_all(message.as_ref())?;
                stderr.flush()?;
            }
            LogOutput::Console { message: _ } => {
                panic!("LogOutput::Console")
            }
            LogOutput::StdIn { message: _ } => panic!("LogOutput::StdIn"),
        }
    }

    // remove the container after the run is finished
    // todo: mount a volume with crates.io index to speed up build in new container on the
    // next run.
    let _ = client
        .remove_container(&container_id, None)
        .await
        .context(format!("Error removing container {}", container_id))?;

    if let Some(build_result) = build_result {
        build_result
    } else {
        Err(anyhow::anyhow!(
            "Failed to read build result from docker build"
        ))
    }
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

/// Pulls the docker image from the registry
async fn pull_image(
    client: Docker,
    image: &str,
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
        while let Some(summary_result) = pull_image_stream.next().await {
            let summary = summary_result?;

            if let Some(progress) = summary.progress {
                // todo: use cursor to overwrite the line
                println!("{}", progress);
            }
        }
    } else {
        while pull_image_stream.next().await.is_some() {}
    }

    Ok(())
}
