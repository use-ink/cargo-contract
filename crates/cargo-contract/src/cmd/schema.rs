use std::{
    fs::File,
    path::PathBuf,
};

use anyhow::{
    anyhow,
    Context,
    Result,
};
use colored::Colorize;
use contract_build::{
    Verbosity,
    VerbosityFlags,
};
use jsonschema::JSONSchema;
use schemars::schema_for;

#[derive(Debug, Clone, Default, clap::ValueEnum)]
#[clap(name = "metadata")]
enum Metadata {
    /// Represents the outer schema format of the contract
    #[clap(name = "outer")]
    #[default]
    /// Represents the inner schema format of the contract.
    /// Contains specification of the ink! contract.
    Outer,
    #[clap(name = "inner")]
    Inner,
}

/// Checks if a contract in the given workspace matches that of a reference contract.
#[derive(Debug, clap::Args)]
pub struct GenerateSchemaCommand {
    /// What type of metadata to generate.
    #[clap(long, value_enum, default_value = "outer")]
    metadata: Metadata,
}

/// Represents the result of schema generation
#[derive(serde::Serialize, serde::Deserialize)]
pub struct GenerationResult {
    pub schema: String,
}

impl GenerateSchemaCommand {
    pub fn run(&self) -> Result<GenerationResult> {
        let schema = match self.metadata {
            Metadata::Outer => schema_for!(ink_metadata::InkProject),
            Metadata::Inner => schema_for!(ink_metadata::ConstructorSpec),
        };
        let pretty_string = serde_json::to_string_pretty(&schema)?;

        Ok(GenerationResult {
            schema: pretty_string,
        })
    }
}

/// Verifies the metadata of the given contract against the schema file.
#[derive(Debug, clap::Args)]
pub struct VerifySchemaCommand {
    /// The path to metadata
    #[clap(long, value_parser)]
    schema: PathBuf,
    /// The .contract path to verify the metadata
    #[clap(name = "bundle", long, value_parser)]
    contract_bundle: Option<PathBuf>,
    /// What type of metadata to verify.
    #[clap(long, conflicts_with = "bundle", value_parser)]
    metadata: Option<PathBuf>,
    /// Denotes if output should be printed to stdout.
    #[clap(flatten)]
    verbosity: VerbosityFlags,
    /// Output the result in JSON format
    #[clap(long, conflicts_with = "verbose")]
    output_json: bool,
}

impl VerifySchemaCommand {
    pub fn run(&self) -> Result<SchemaVerificationResult> {
        let verbosity: Verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;

        let mut metadata = serde_json::Value::Null;
        let mut metadata_source = String::new();

        // 1a. Extract given metadata from .contract bundle
        if let Some(path) = &self.contract_bundle {
            let file = File::open(path)
                .context(format!("Failed to open contract bundle {}", path.display()))?;

            let mut contract_metadata: contract_metadata::ContractMetadata =
                serde_json::from_reader(&file).context(format!(
                    "Failed to deserialize contract bundle {}",
                    path.display()
                ))?;
            contract_metadata.remove_source_wasm_attribute();

            metadata = serde_json::value::to_value(contract_metadata)?;
            metadata_source = path.display().to_string();
        }

        // 1a. Read metadata file
        if let Some(path) = &self.metadata {
            let file = File::open(path)
                .context(format!("Failed to open metadata file {}", path.display()))?;

            let contract_metadata: contract_metadata::ContractMetadata =
                serde_json::from_reader(&file).context(format!(
                    "Failed to deserialize metadata file {}",
                    path.display()
                ))?;

            metadata = serde_json::value::to_value(contract_metadata)?;
            metadata_source = path.display().to_string();
        }

        // 2. Open schema file
        let path = &self.schema;
        let file = File::open(path)
            .context(format!("Failed to open schema file {}", path.display()))?;

        let schema: serde_json::Value = serde_json::from_reader(&file).context(
            format!("Failed to deserialize schema file {}", path.display()),
        )?;

        // 3. Build validator

        // We have to use let-else here, otherwise `&schema` is required to be static
        let Ok(validator) = JSONSchema::compile(&schema) else {
            anyhow::bail!("Failed to compile schema to validation tree")
        };

        // 3. Validate and display error if any
        validator.validate(&metadata).map_err(|errors| {
            let error_msg = errors.fold(
                String::from("Error during schema validation:\n"),
                |acc, e| format!("{}\n{}", acc, e),
            );
            anyhow!(error_msg)
        })?;

        Ok(SchemaVerificationResult {
            is_verified: true,
            metadata_source,
            schema: self.schema.display().to_string(),
            output_json: self.output_json,
            verbosity,
        })
    }
}

/// The result of verification process
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SchemaVerificationResult {
    pub is_verified: bool,
    pub metadata_source: String,
    pub schema: String,
    #[serde(skip_serializing, skip_deserializing)]
    pub output_json: bool,
    #[serde(skip_serializing, skip_deserializing)]
    pub verbosity: Verbosity,
}

impl SchemaVerificationResult {
    /// Display the result in a fancy format
    pub fn display(&self) -> String {
        format!(
            "\n{} {} against schema {}",
            "Successfully verified metadata in".bright_green().bold(),
            format!("`{}`", &self.metadata_source).bold(),
            format!("`{}`!", &self.schema).bold()
        )
    }

    /// Display the build results in a pretty formatted JSON string.
    pub fn serialize_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}
