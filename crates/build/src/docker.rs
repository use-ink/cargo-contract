use std::path::Path;

use anyhow::{
    Context,
    Result,
};
use bollard::{
    container::{
        Config,
        CreateContainerOptions,
        WaitContainerOptions,
    },
    image::ListImagesOptions,
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
    BuildMode,
    BuildResult,
    BuildSteps,
    CrateMetadata,
    ExecuteArgs,
    Network,
    OptimizationPasses,
    DEFAULT_MAX_MEMORY_PAGES,
};

use colored::Colorize;

pub fn docker_build(args: ExecuteArgs) -> Result<BuildResult> {
    let ExecuteArgs {
        manifest_path,
        verbosity,
        features,
        build_mode,
        network,
        unstable_flags,
        optimization_passes,
        keep_debug_symbols,
        output_type,
        target,
        max_memory_pages,
        build_artifact,
        ..
    } = args;
    tokio::runtime::Runtime::new()?.block_on(async {
        let mut build_steps = BuildSteps::new();

        build_steps.set_total_steps(4);
        if build_artifact == BuildArtifacts::CodeOnly {
            build_steps.set_total_steps(3);
        }

        maybe_println!(
            verbosity,
            " {} {}",
            format!("{build_steps}").bold(),
            "Executing verifiable build".bright_green().bold()
        );

        let client = Docker::connect_with_socket_defaults()?;

        let images = client
            .list_images(Some(ListImagesOptions::<String> {
                all: true,
                ..Default::default()
            }))
            .await
            .unwrap();

        let build_image = images.iter().find(|i| {
            i.labels.get("io.parity.image.title") == Some(&"/ink-build-image".to_string())
        });
        if build_image.is_none() {
            return Err(anyhow::anyhow!("No image found"))
        }
        let build_image = build_image.unwrap();

        let options = Some(CreateContainerOptions {
            name: "ink-container",
            platform: Some("linux/amd64"),
        });

        let crate_metadata = CrateMetadata::collect(&manifest_path, target)?;
        let host_folder = crate_metadata
            .manifest_path
            .absolute_directory()?
            .as_path()
            .to_owned();

        let file_path = host_folder.join(Path::new("target/build_result.json"));

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
        println!("Mount cfg: {:?}", &mount);
        let host_cfg = Some(HostConfig {
            mounts: Some(vec![mount]),
            ..Default::default()
        });

        let entrypoint = vec!["/bin/bash", "-c"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let mut args: Vec<String> = Vec::new();

        features.append_to_args(&mut args);
        if keep_debug_symbols {
            args.push("--keep-debug-symbols".to_owned());
        }

        if let Some(passes) = optimization_passes {
            if passes != OptimizationPasses::default() {
                args.push(format!("--optimization-passes {}", passes));
            }
        }

        if network == Network::Offline {
            args.push("--offline".to_owned());
        }

        if build_mode == BuildMode::Release {
            args.push("--release".to_owned());
        }

        if unstable_flags.original_manifest {
            args.push("-Z original-manifest".to_owned());
        }

        let s = match build_artifact {
            BuildArtifacts::CodeOnly => "--generate code-only".to_owned(),
            BuildArtifacts::All => String::new(),
            BuildArtifacts::CheckOnly => {
                anyhow::bail!("--generate check-only is invalid flag for this command!");
            }
        };

        args.push(s);

        if max_memory_pages != DEFAULT_MAX_MEMORY_PAGES {
            args.push(format!("--max-memory-pages {}", max_memory_pages));
        }

        let joined_args = args.join(" ");

        let cmds = vec![format!(
            "cargo contract build {} --output-json > target/build_result.json",
            joined_args
        )];

        let config = Config {
            image: Some(build_image.id.clone()),
            entrypoint: Some(entrypoint),
            cmd: Some(cmds),
            host_config: host_cfg,
            attach_stderr: Some(true),
            ..Default::default()
        };

        let container_id = match client
            .create_container(options.clone(), config.clone())
            .await
        {
            Ok(response) => response.id,
            Err(_) => {
                // container might exist, so we delete the previous one
                let _ = client.remove_container("ink-container", None).await;
                client.create_container(options, config).await?.id
            }
        };
        match client.start_container::<String>(&container_id, None).await {
            Ok(_) => {
                // TODO: message
            }
            Err(e) => {
                // TODO: message
                let _ = client.remove_container(&container_id, None).await;
                let _ = std::fs::remove_file(&file_path);
                anyhow::bail!(e)
            }
        }

        build_steps.increment_current();

        maybe_println!(
            verbosity,
            " {} {}\n{}",
            format!("{build_steps}").bold(),
            "Started the build inside the container"
                .bright_green()
                .bold(),
            "This might take a while. Check the docker engine for the logs."
        );

        let options = Some(WaitContainerOptions {
            condition: "not-running",
        });

        let mut wait_stream = client.wait_container(&container_id, options);
        while wait_stream.next().await.is_some() {}

        let _ = client.remove_container(&container_id, None).await;

        build_steps.increment_current();
        maybe_println!(
            verbosity,
            " {} {}",
            format!("{build_steps}").bold(),
            "Docker container has finished the build."
                .bright_green()
                .bold(),
        );

        let result_contents = match std::fs::read_to_string(&file_path) {
            Ok(content) => {
                std::fs::remove_file(&file_path)?;
                content
            }
            Err(e) => {
                std::fs::remove_file(&file_path)?;
                anyhow::bail!(e);
            }
        };

        let mut build_result: BuildResult = serde_json::from_str(&result_contents)
            .map_err(|_| {
                anyhow::anyhow!(
                    "Error parsing output from docker build. The build probably failed!"
                )
            })?;

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

        if let Some(metadata_artifacts) = &build_result.metadata_result {
            let mut metadata = ContractMetadata::load(&metadata_artifacts.dest_bundle)?;
            metadata.image = Some(build_image.id.to_string());

            crate::metadata::write_metadata(
                metadata_artifacts,
                metadata,
                &mut build_steps,
                &verbosity,
            )?;
        }

        Ok(BuildResult {
            output_type,
            verbosity,
            ..build_result
        })
    })
}
