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

use std::{
    collections::{
        hash_map::DefaultHasher,
        HashMap,
    },
    hash::{
        Hash,
        Hasher,
    },
    io::BufReader,
    path::{
        Path,
        PathBuf,
    },
    time::Duration,
};

use anyhow::{
    Context,
    Result,
};
use bollard::{
    container::{
        Config,
        CreateContainerOptions,
        ListContainersOptions,
        StopContainerOptions,
        WaitContainerOptions,
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
};

use colored::Colorize;

const IMAGE: &str = "paritytech/contracts-verifiable";
// We assume the docker image contains the same tag as the current version of the crate
const VERSION: &str = env!("CARGO_PKG_VERSION");

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
            let host_folder = crate_metadata
                .manifest_path
                .absolute_directory()?
                .as_path()
                .to_owned();
            let file_path = host_folder.join(Path::new("target/build_result.json"));
            let args = compose_build_args()?;

            let image_variant = match image {
                Some(i) => i,
                None => ImageVariant::Default,
            };

            let client = Docker::connect_with_socket_defaults()?;
            let build_image =
                get_image(client.clone(), image_variant, &verbosity, &mut build_steps)
                    .await?;

            run_build(
                args,
                &build_image,
                &crate_metadata.contract_artifact_name,
                &host_folder,
                &verbosity,
                &mut build_steps,
            )
            .await?;

            let build_result = read_build_result(&host_folder, &file_path)?;

            update_metadata(&build_result, &verbosity, &mut build_steps, &build_image)?;

            Ok(BuildResult {
                output_type,
                verbosity,
                ..build_result
            })
        })
}

/// Reads the `BuildResult` produced by the docker execution
fn read_build_result(host_folder: &Path, file_path: &PathBuf) -> Result<BuildResult> {
    let file = std::fs::File::open(file_path)?;
    let mut build_result: BuildResult =
        match serde_json::from_reader(BufReader::new(file)) {
            Ok(result) => result,
            Err(_) => {
                std::fs::remove_file(file_path)?;
                anyhow::bail!(
                    "Error parsing output from docker build. The build probably failed!"
                )
            }
        };

    let new_path = host_folder.join(
        build_result
            .target_directory
            .as_path()
            .strip_prefix("/contract")?,
    );
    build_result.target_directory = new_path;

    let new_path = build_result.dest_wasm.as_ref().map(|p| {
        host_folder.join(
            p.as_path()
                .strip_prefix("/contract")
                .expect("cannot strip prefix"),
        )
    });
    build_result.dest_wasm = new_path;

    build_result.metadata_result.as_mut().map(|mut m| {
        m.dest_bundle = host_folder.join(
            m.dest_bundle
                .as_path()
                .strip_prefix("/contract")
                .expect("cannot strip prefix"),
        );
        m.dest_metadata = host_folder.join(
            m.dest_metadata
                .as_path()
                .strip_prefix("/contract")
                .expect("cannot strip prefix"),
        );
        m
    });
    Ok(build_result)
}

/// Overwrites `build_result` and `image` fields in the metadata
fn update_metadata(
    build_result: &BuildResult,
    verbosity: &Verbosity,
    build_steps: &mut BuildSteps,
    build_image: &ImageSummary,
) -> Result<()> {
    if let Some(metadata_artifacts) = &build_result.metadata_result {
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
            metadata_artifacts,
            metadata,
            build_steps,
            verbosity,
        )?;
    }
    Ok(())
}

/// Creates the container and executed the build inside it
async fn run_build(
    build_args: String,
    build_image: &ImageSummary,
    contract_name: &str,
    host_folder: &Path,
    verbosity: &Verbosity,
    build_steps: &mut BuildSteps,
) -> Result<()> {
    let client = Docker::connect_with_socket_defaults()?;

    let mut entrypoint = vec!["/bin/bash".to_string(), "-c".to_string()];

    let mut cmds = vec![format!(
            "mkdir -p target && cargo contract build {} --output-json > target/build_result.json",
            build_args
        )];

    entrypoint.append(&mut cmds);

    // in order to optimise the container usage
    // we are hashing the inputted command
    // in order to reuse the container for the same permutation of arguments
    let mut s = DefaultHasher::new();
    entrypoint.hash(&mut s);
    let digest = s.finish();
    // taking the first 5 digits to be a unique identifier
    let digest_code: String = digest.to_string().chars().take(5).collect();

    let container_name = format!("ink-verified-{}-{}", contract_name, digest_code);

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
        target: Some(String::from("/contract")),
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

    let mut labels = HashMap::new();
    labels.insert("cmd_digest".to_string(), digest.to_string());
    let config = Config {
        image: Some(build_image.id.clone()),
        entrypoint: Some(entrypoint),
        cmd: None,
        labels: Some(labels),
        host_config: host_cfg,
        attach_stderr: Some(true),
        tty: Some(true),
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
        " {} {}\n {}",
        format!("{build_steps}").bold(),
        "Started the build inside the container"
            .bright_green()
            .bold(),
        "You can close this terminal session. The execution will be finished in the background"
    );

    let options = Some(WaitContainerOptions {
        condition: "not-running",
    });

    let mut wait_stream = client.wait_container(&container_id, options);
    if verbosity.is_verbose() {
        let spinner_style =
            ProgressStyle::with_template(" {spinner:.cyan.bold} {wide_msg}")?
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ");

        let pb = ProgressBar::new(1000);
        pb.enable_steady_tick(Duration::from_millis(100));
        pb.set_style(spinner_style);
        pb.set_message("Build is being executed...");
        while wait_stream.next().await.is_some() {
            pb.inc(1);
        }
        pb.finish_with_message("Done!")
    } else {
        while wait_stream.next().await.is_some() {}
    }

    client
        .stop_container(&container_id, Some(StopContainerOptions { t: 20 }))
        .await?;

    build_steps.increment_current();
    maybe_println!(
        verbosity,
        " {} {}",
        format!("{build_steps}").bold(),
        "Docker container has finished the build. Reading the results"
            .bright_green()
            .bold(),
    );
    Ok(())
}

/// Takes CLI args from the host and appends them to the build command inside the docker
fn compose_build_args() -> Result<String> {
    let mut args: Vec<String> = vec!["--release".to_string()];

    let mut os_args: Vec<String> = std::env::args()
        .filter(|a| {
            a != "--verifiable"
                && a != "--image"
                && !a.contains("cargo-contract")
                && a != "cargo"
                && a != "contract"
                && a != "build"
        })
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
