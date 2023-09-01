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
    cmp::Ordering,
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
    marker::Unpin,
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
        ListContainersOptions,
        LogOutput,
    },
    errors::Error,
    image::{
        CreateImageOptions,
        ListImagesOptions,
    },
    models::CreateImageInfo,
    service::{
        HostConfig,
        ImageSummary,
        Mount,
        MountTypeEnum,
    },
    Docker,
};
use contract_metadata::ContractMetadata;
use tokio_stream::{
    Stream,
    StreamExt,
};

use crate::{
    verbose_eprintln,
    BuildResult,
    CrateMetadata,
    ExecuteArgs,
    Verbosity,
};

use colored::Colorize;
/// Default image to be used for the build.
const IMAGE: &str = "paritytech/contracts-verifiable";
/// We assume the docker image contains the same tag as the current version of the crate.
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The default directory to be mounted in the container.
const MOUNT_DIR: &str = "/contract";

/// The image to be used.
#[derive(Clone, Debug, Default)]
pub enum ImageVariant {
    /// The default image is used, specified in the `IMAGE` constant.
    #[default]
    Default,
    /// Custom image is used.
    Custom(String),
}

impl From<Option<String>> for ImageVariant {
    fn from(value: Option<String>) -> Self {
        if let Some(image) = value {
            ImageVariant::Custom(image)
        } else {
            ImageVariant::Default
        }
    }
}

/// Launches the docker container to execute verifiable build.
pub fn docker_build(args: ExecuteArgs) -> Result<BuildResult> {
    let ExecuteArgs {
        manifest_path,
        verbosity,
        output_type,
        target,
        image,
        ..
    } = args;
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let crate_metadata = CrateMetadata::collect(&manifest_path, target)?;
            let host_folder = std::env::current_dir()?;
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

            let container = create_container(
                &client,
                args.clone(),
                &image,
                &crate_metadata.contract_artifact_name,
                &host_folder,
                &verbosity,
            )
            .await?;

            let mut build_result = run_build(&client, &container, &verbosity).await?;

            update_build_result(&host_folder, &mut build_result)?;

            update_metadata(&build_result, &verbosity, &image, &client).await?;

            verbose_eprintln!(
                verbosity,
                " {} {}",
                "[==]".bold(),
                "Displaying results".bright_cyan().bold(),
            );

            Ok(BuildResult {
                output_type,
                verbosity,
                ..build_result
            })
        })
}

/// Updates `build_result` paths to the artefacts.
fn update_build_result(host_folder: &Path, build_result: &mut BuildResult) -> Result<()> {
    let new_path = host_folder.join(
        build_result
            .target_directory
            .as_path()
            .strip_prefix(MOUNT_DIR)?,
    );
    build_result.target_directory = new_path;

    let new_path = build_result.dest_wasm.as_ref().map(|p| {
        host_folder.join(
            p.as_path()
                .strip_prefix(MOUNT_DIR)
                .expect("cannot strip prefix"),
        )
    });
    build_result.dest_wasm = new_path;

    build_result.metadata_result.as_mut().map(|m| {
        m.dest_bundle = host_folder.join(
            m.dest_bundle
                .as_path()
                .strip_prefix(MOUNT_DIR)
                .expect("cannot strip prefix"),
        );
        m.dest_metadata = host_folder.join(
            m.dest_metadata
                .as_path()
                .strip_prefix(MOUNT_DIR)
                .expect("cannot strip prefix"),
        );
        m
    });
    Ok(())
}

/// Overwrites `build_result` and `image` fields in the metadata.
async fn update_metadata(
    build_result: &BuildResult,
    verbosity: &Verbosity,
    build_image: &str,
    client: &Docker,
) -> Result<()> {
    if let Some(metadata_artifacts) = &build_result.metadata_result {
        let mut metadata = ContractMetadata::load(&metadata_artifacts.dest_bundle)?;

        let build_image = find_local_image(client, build_image.to_string())
            .await?
            .context("Image summary does not exist")?;
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

        crate::metadata::write_metadata(metadata_artifacts, metadata, verbosity, true)?;
    }
    Ok(())
}

/// Searches for the local copy of the docker image.
async fn find_local_image(
    client: &Docker,
    image: String,
) -> Result<Option<ImageSummary>> {
    let images = client
        .list_images(Some(ListImagesOptions::<String> {
            all: true,
            ..Default::default()
        }))
        .await?;
    let build_image = images.iter().find(|i| i.repo_tags.contains(&image));

    Ok(build_image.cloned())
}

/// Creates the container, returning the container id if successful.
///
/// If the image is not available locally, it will be pulled from the registry.
async fn create_container(
    client: &Docker,
    mut build_args: Vec<String>,
    build_image: &str,
    contract_name: &str,
    host_folder: &Path,
    verbosity: &Verbosity,
) -> Result<String> {
    let entrypoint = vec!["cargo".to_string(), "contract".to_string()];

    let mut cmd = vec![
        "build".to_string(),
        "--release".to_string(),
        "--output-json".to_string(),
    ];

    cmd.append(&mut build_args);

    let digest_code = container_digest(cmd.clone(), build_image.to_string());
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

    if container_option.is_some() {
        return Ok(container_name)
    }

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
        host_config: host_cfg,
        attach_stderr: Some(true),
        user,
        ..Default::default()
    };
    let options = Some(CreateContainerOptions {
        name: container_name.as_str(),
        platform: Some("linux/amd64"),
    });

    match client
        .create_container(options.clone(), config.clone())
        .await
    {
        Ok(_) => Ok(container_name),
        Err(err) => {
            if matches!(
                err,
                bollard::errors::Error::DockerResponseServerError {
                    status_code: 404,
                    ..
                }
            ) {
                // no such image locally, so pull and try again
                pull_image(client, build_image.to_string(), verbosity).await?;
                client
                    .create_container(options, config)
                    .await
                    .context("Failed to create docker container")
                    .map(|_| container_name)
            } else {
                Err(err.into())
            }
        }
    }
}

/// Starts the container and executed the build inside it.
async fn run_build(
    client: &Docker,
    container_name: &str,
    verbosity: &Verbosity,
) -> Result<BuildResult> {
    client
        .start_container::<String>(container_name, None)
        .await?;

    let AttachContainerResults { mut output, .. } = client
        .attach_container(
            container_name,
            Some(AttachContainerOptions::<String> {
                stdout: Some(true),
                stderr: Some(true),
                stream: Some(true),
                ..Default::default()
            }),
        )
        .await?;

    verbose_eprintln!(
        verbosity,
        " {} {}",
        "[==]".bold(),
        format!("Started the build inside the container: {}", container_name)
            .bright_cyan()
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
                    serde_json::from_reader(BufReader::new(message.as_ref())).context(
                        format!(
                            "Error decoding BuildResult:\n {}",
                            std::str::from_utf8(&message).unwrap()
                        ),
                    ),
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

    if let Some(build_result) = build_result {
        build_result
    } else {
        Err(anyhow::anyhow!(
            "Failed to read build result from docker build"
        ))
    }
}

/// Takes CLI args from the host and appends them to the build command inside the docker.
fn compose_build_args() -> Result<Vec<String>> {
    use regex::Regex;
    let mut args: Vec<String> = Vec::new();
    // match `--image` or `verify` with arg with 1 or more white spaces surrounded
    let rex = Regex::new(r#"(--image|verify)[ ]*[^ ]*[ ]*"#)?;
    // we join the args together, so we can remove `--image <arg>`
    let args_string: String = std::env::args().collect::<Vec<String>>().join(" ");
    let args_string = rex.replace_all(&args_string, "").to_string();

    // and then we turn it back to the vec, filtering out commands and arguments
    // that should not be passed to the docker build command
    let mut os_args: Vec<String> = args_string
        .split_ascii_whitespace()
        .filter(|a| {
            a != &"--verifiable"
                && !a.contains("cargo-contract")
                && a != &"cargo"
                && a != &"contract"
                && a != &"build"
                && a != &"--output-json"
        })
        .map(|s| s.to_string())
        .collect();

    args.append(&mut os_args);

    Ok(args)
}

/// Pulls the docker image from the registry.
async fn pull_image(client: &Docker, image: String, verbosity: &Verbosity) -> Result<()> {
    let mut pull_image_stream = client.create_image(
        Some(CreateImageOptions {
            from_image: image,
            ..Default::default()
        }),
        None,
        None,
    );

    verbose_eprintln!(
        verbosity,
        " {} {}",
        "[==]".bold(),
        "Image does not exist. Pulling one from the registry"
            .bright_cyan()
            .bold()
    );

    if verbosity.is_verbose() {
        show_pull_progress(pull_image_stream).await?
    } else {
        while pull_image_stream.next().await.is_some() {}
    }

    Ok(())
}

/// Display the progress of the pulling of each image layer.
async fn show_pull_progress(
    mut pull_image_stream: impl Stream<Item = Result<CreateImageInfo, Error>> + Sized + Unpin,
) -> Result<()> {
    use crossterm::{
        cursor,
        terminal::{
            self,
            ClearType,
        },
    };

    let mut layers = Vec::new();
    let mut curr_index = 0i16;
    while let Some(result) = pull_image_stream.next().await {
        let info = result?;

        let status = info.status.unwrap_or_default();
        if status.starts_with("Digest:") || status.starts_with("Status:") {
            eprintln!("{}", status);
            continue
        }

        if let Some(id) = info.id {
            let mut move_cursor = String::new();
            if let Some(index) = layers.iter().position(|l| l == &id) {
                let index = index + 1;
                let diff = index as i16 - curr_index;
                curr_index = index as i16;
                match diff.cmp(&1) {
                    Ordering::Greater => {
                        let down = diff - 1;
                        move_cursor = format!("{}", cursor::MoveDown(down as u16))
                    }
                    Ordering::Less => {
                        let up = diff.abs() + 1;
                        move_cursor = format!("{}", cursor::MoveUp(up as u16))
                    }
                    Ordering::Equal => {}
                }
            } else {
                layers.push(id.clone());
                let len = layers.len() as i16;
                let diff = len - curr_index;
                curr_index = len;
                if diff > 1 {
                    move_cursor = format!("{}", cursor::MoveDown(diff as u16))
                }
            };

            let clear_line = terminal::Clear(ClearType::CurrentLine);

            if status == "Pull complete" {
                eprintln!("{}{}{}: {}", move_cursor, clear_line, id, status)
            } else {
                let progress = info.progress.unwrap_or_default();
                eprintln!(
                    "{}{}{}: {} {}",
                    move_cursor, clear_line, id, status, progress
                )
            }
        }
    }
    Ok(())
}

/// Calculates the unique container's code.
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
