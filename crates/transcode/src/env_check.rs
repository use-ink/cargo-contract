use ink_metadata::InkProject;
use scale_info::{
    form::PortableForm,
    Field,
    PortableRegistry,
    TypeDef,
};

use anyhow::{
    Context,
    Result,
};

#[derive(Debug, Clone)]
pub enum EnvCheckError {
    CheckFailed(String),
    RegistryError(String),
}

impl std::fmt::Display for EnvCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CheckFailed(e) => {
                f.write_fmt(format_args!(
                    "Type check failed with following error: {}",
                    e
                ))
            }
            Self::RegistryError(e) => {
                f.write_fmt(format_args!(
                    "Error occurred while parsing type registry: {}",
                    e
                ))
            }
        }
    }
}

impl std::error::Error for EnvCheckError {}

impl From<anyhow::Error> for EnvCheckError {
    fn from(value: anyhow::Error) -> Self {
        Self::RegistryError(value.to_string())
    }
}

fn get_node_env_fields(registry: &PortableRegistry) -> Result<Vec<Field<PortableForm>>> {
    let env_type = registry
        .types
        .iter()
        .find(|t| t.ty.path.segments == ["pallet_contracts", "Environment"])
        .context("The node does not contain `Environment` type. Are you using correct `pallet-contracts` version?")?;

    if let TypeDef::Composite(composite) = &env_type.ty.type_def {
        Ok(composite.fields.clone())
    } else {
        anyhow::bail!("`Environment` type definition is in the wrong format");
    }
}

pub(crate) fn resolve_type_definition(
    registry: &PortableRegistry,
    id: u32,
) -> Result<TypeDef<PortableForm>> {
    let tt = registry
        .resolve(id)
        .context("Type is not present in registry")?;
    if tt.type_params.is_empty() {
        if let TypeDef::Composite(comp) = &tt.type_def {
            let tt_id = comp
                .fields
                .get(0)
                .context("Incorrect format of a field")?
                .ty
                .id;
            return resolve_type_definition(registry, tt_id)
        }
        Ok(tt.type_def.clone())
    } else {
        let param_id = tt
            .type_params
            .get(0)
            .context("type param is not present")?
            .ty
            .context("concrete type is not present")?
            .id;
        resolve_type_definition(registry, param_id)
    }
}

/// Compares the environment type of the targeted chain against the current contract.
///
/// It is achieved by iterating over the type specifications of `Environment` trait
/// in the node's metadata anf comparing finding the corresponding type
/// in the contract's `Environment` trait.
pub fn compare_node_env_with_contract(
    node_registry: &PortableRegistry,
    contract_metadata: &InkProject,
) -> Result<(), EnvCheckError> {
    let env_fields = get_node_env_fields(node_registry)?;
    for field in env_fields {
        let field_name = field.name.context("Field does not have a name")?;
        if &field_name == "hasher" {
            continue
        }
        let field_def = resolve_type_definition(node_registry, field.ty.id)?;
        let checked =
            compare_type(&field_name, field_def, contract_metadata, node_registry)?;
        if !checked {
            return Err(EnvCheckError::CheckFailed(field_name))
        }
    }
    Ok(())
}

/// Compares the contract's environment type with a provided type definition.
fn compare_type(
    type_name: &str,
    type_def: TypeDef<PortableForm>,
    contract_metadata: &InkProject,
    node_registry: &PortableRegistry,
) -> Result<bool> {
    let contract_registry = contract_metadata.registry();
    let tt_id = match type_name {
        "account_id" => contract_metadata.spec().environment().account_id().ty().id,
        "balance" => contract_metadata.spec().environment().balance().ty().id,
        "hash" => contract_metadata.spec().environment().hash().ty().id,
        "timestamp" => contract_metadata.spec().environment().timestamp().ty().id,
        "block_number" => {
            contract_metadata
                .spec()
                .environment()
                .block_number()
                .ty()
                .id
        }
        _ => anyhow::bail!("Trying to resolve unknown environment type"),
    };
    let tt_def = resolve_type_definition(contract_registry, tt_id)?;
    if let TypeDef::Array(node_arr) = &type_def {
        let node_arr_type =
            resolve_type_definition(node_registry, node_arr.type_param.id)?;
        if let TypeDef::Array(contract_arr) = &tt_def {
            if node_arr.len != contract_arr.len {
                anyhow::bail!("Mismatch in array lengths");
            }
            let contract_arr_type =
                resolve_type_definition(contract_registry, contract_arr.type_param.id)?;
            return Ok(contract_arr_type == node_arr_type)
        }
    }
    Ok(type_def == tt_def)
}
