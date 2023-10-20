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

use crate::ContractMessageTranscoder;

#[derive(Debug, Clone)]
pub enum EnvCheckError {
    CheckFailed(String),
    RegistryError(String),
}

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

pub fn resolve_type_definition(
    registry: &PortableRegistry,
    id: u32,
) -> Result<TypeDef<PortableForm>> {
    let tt = registry
        .resolve(id)
        .context("Type is not present in registry")?;
    if tt.type_params.is_empty() {
        if let TypeDef::Composite(comp) = &tt.type_def {
            println!("Resolve type definition: {:#?}", tt);
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

pub fn compare_node_env_with_contract(
    registry: &PortableRegistry,
    transcoder: &ContractMessageTranscoder,
) -> Result<(), EnvCheckError> {
    let env_fields = get_node_env_fields(registry)?;
    for field in env_fields {
        let field_name = field.name.context("Field does not have a name")?;
        if &field_name == "hasher" {
            continue
        }
        let field_def = resolve_type_definition(registry, field.ty.id)?;
        let checked = transcoder.compare_type(&field_name, field_def, registry)?;
        if !checked {
            return Err(EnvCheckError::CheckFailed(field_name))
        }
    }
    Ok(())
}
