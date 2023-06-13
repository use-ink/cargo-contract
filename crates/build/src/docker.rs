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
    image::ListImagesOptions,
    service::{
        HostConfig,
        ImageSummary,
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
    Features,
    Network,
    OptimizationPasses,
    UnstableFlags,
    Verbosity,
    DEFAULT_MAX_MEMORY_PAGES,
};

use colored::Colorize;

#[derive(Clone, Debug, Default)]
pub enum ImageVariant {
    #[default]
    Default,
    Custom(String),
}

/// Launched the docker container to execute verifiable build
pub fn docker_build(args: ExecuteArgs) -> Result<BuildResult> {
    let ExecuteArgs {
        manifest_path,
        verbosity,
        mut features,
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
        let args = compose_build_args(
            &mut features,
            keep_debug_symbols,
            optimization_passes.as_ref(),
            &network,
            &unstable_flags,
            &build_artifact,
            max_memory_pages,
        )?;

        // TODO: replace with image pulling, once the image is pushed to registry
        let client = Docker::connect_with_socket_defaults()?;
        let images = client
            .list_images(Some(ListImagesOptions::<String> {
                all: true,
                ..Default::default()
            }))
            .await?;
        let build_image = images.iter().find(|i| {
            i.labels.get("io.parity.image.title")
                == Some(&"contracts-verifiable".to_string())
        });
        if build_image.is_none() {
            return Err(anyhow::anyhow!("No image found"))
        }
        let build_image = build_image.unwrap();

        run_build(
            args,
            build_image,
            &crate_metadata.contract_artifact_name,
            &host_folder,
            &verbosity,
            &mut build_steps,
        )
        .await?;

        let build_result = read_build_result(&host_folder, &file_path)?;

        update_metadata(&build_result, &verbosity, &mut build_steps, build_image)?;

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

fn update_metadata(
    build_result: &BuildResult,
    verbosity: &Verbosity,
    build_steps: &mut BuildSteps,
    build_image: &ImageSummary,
) -> Result<()> {
    if let Some(metadata_artifacts) = &build_result.metadata_result {
        let mut metadata = ContractMetadata::load(&metadata_artifacts.dest_bundle)?;
        metadata.image = Some(build_image.id.to_string());

        crate::metadata::write_metadata(
            metadata_artifacts,
            metadata,
            build_steps,
            verbosity,
        )?;
    }
    Ok(())
}

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
        " {} {}\n{}",
        format!("{build_steps}").bold(),
        "Started the build inside the container"
            .bright_green()
            .bold(),
        "This might take a while. Check container logs for more details."
    );

    let options = Some(WaitContainerOptions {
        condition: "not-running",
    });

    let mut wait_stream = client.wait_container(&container_id, options);
    while wait_stream.next().await.is_some() {}

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

fn compose_build_args(
    features: &mut Features,
    keep_debug_symbols: bool,
    optimization_passes: Option<&OptimizationPasses>,
    network: &Network,
    unstable_flags: &UnstableFlags,
    build_artifact: &BuildArtifacts,
    max_memory_pages: u32,
) -> Result<String> {
    let mut args: Vec<String> = vec!["--release".to_string()];
    features.append_to_args(&mut args);
    if keep_debug_symbols {
        args.push("--keep-debug-symbols".to_owned());
    }

    if let Some(passes) = optimization_passes {
        if passes != &OptimizationPasses::default() {
            args.push(format!("--optimization-passes {}", passes));
        }
    }

    if network == &Network::Offline {
        args.push("--offline".to_owned());
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
    Ok(joined_args)
}
