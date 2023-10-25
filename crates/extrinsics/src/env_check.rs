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

fn get_node_env_fields(
    registry: &PortableRegistry,
) -> Result<Option<Vec<Field<PortableForm>>>> {
    let Some(env_type) = registry.types.iter().find(|t| {
        let len = t.ty.path.segments.len();
        t.ty.path.segments[len - 2..] == ["pallet_contracts", "Environment"]
    }) else {
        // if we can't find the type, then we use the old contract version.
        contract_build::verbose_eprintln!(true, "The node does not contain `Environment` type. Are you using correct `pallet-contracts` version?");
        return Ok(None)
    };

    if let TypeDef::Composite(composite) = &env_type.ty.type_def {
        Ok(Some(composite.fields.clone()))
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
            if comp.fields.len() > 1 || comp.fields.is_empty() {
                anyhow::bail!("Composite field has incorrect composite type format")
            }

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
) -> Result<()> {
    let Some(env_fields) = get_node_env_fields(node_registry)? else {
        return Ok(())
    };
    for field in env_fields {
        let field_name = field.name.context("Field does not have a name")?;
        if &field_name == "hasher" {
            continue
        }
        let field_def = resolve_type_definition(node_registry, field.ty.id)?;
        let checked =
            compare_type(&field_name, field_def, contract_metadata, node_registry)?;
        if !checked {
            anyhow::bail!("Failed to validate the field: {}", field_name);
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

#[cfg(test)]
mod tests {
    use ink_metadata::{
        layout::{
            Layout,
            LayoutKey,
            LeafLayout,
        },
        ConstructorSpec,
        ContractSpec,
        EnvironmentSpec,
        InkProject,
        MessageParamSpec,
        MessageSpec,
        ReturnTypeSpec,
        TypeSpec,
    };
    use scale::{
        Decode,
        Encode,
    };
    use scale_info::{
        form::PortableForm,
        MetaType,
        PortableRegistry,
        Registry,
        TypeDef,
        TypeInfo,
    };
    use std::marker::PhantomData;

    use crate::{
        compare_node_env_with_contract,
        env_check::resolve_type_definition,
    };

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    pub struct AccountId([u8; 32]);

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    pub struct Balance(u128);

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    pub struct Hash([u8; 32]);

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    pub struct Hasher;

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    pub struct Timestamp(u64);

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    pub struct BlockNumber(u32);

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    pub struct SomeStruct {
        one: u32,
        two: u64,
    }

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    pub struct CompositeBlockNumber(SomeStruct);

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    pub struct EnvironmentType<T>(PhantomData<T>);

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    #[scale_info(replace_segment("tests", "pallet_contracts"))]
    pub struct Environment {
        account_id: EnvironmentType<AccountId>,
        balance: EnvironmentType<Balance>,
        hash: EnvironmentType<Hash>,
        hasher: EnvironmentType<Hasher>,
        timestamp: EnvironmentType<Timestamp>,
        block_number: EnvironmentType<BlockNumber>,
    }

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    #[scale_info(replace_segment("tests", "pallet_contracts"))]
    #[scale_info(replace_segment("InvalidEnvironment", "Environment"))]
    pub struct InvalidEnvironment {
        account_id: EnvironmentType<AccountId>,
        balance: EnvironmentType<Balance>,
        hash: EnvironmentType<Hash>,
        hasher: EnvironmentType<Hasher>,
        timestamp: EnvironmentType<Timestamp>,
        block_number: EnvironmentType<CompositeBlockNumber>,
    }

    #[test]
    fn resolve_works() {
        let mut registry = Registry::new();
        registry.register_type(&MetaType::new::<Environment>());
        let u64_typedef =
            TypeDef::<PortableForm>::Primitive(scale_info::TypeDefPrimitive::U64);

        let portable: PortableRegistry = registry.into();
        let resolved_type = resolve_type_definition(&portable, 12);
        assert!(resolved_type.is_ok());
        let resolved_type = resolved_type.unwrap();

        assert_eq!(u64_typedef, resolved_type);
    }

    #[test]
    #[should_panic(expected = "Type is not present in registry")]
    fn resolve_unknown_type_fails() {
        let mut registry = Registry::new();
        registry.register_type(&MetaType::new::<Environment>());

        let portable: PortableRegistry = registry.into();
        let _ = resolve_type_definition(&portable, 18).unwrap();
    }

    #[test]
    #[should_panic(expected = "Composite field has incorrect composite type format")]
    fn composite_type_fails_to_resolve() {
        let mut registry = Registry::new();
        registry.register_type(&MetaType::new::<InvalidEnvironment>());

        let portable: PortableRegistry = registry.into();
        let _ = resolve_type_definition(&portable, 15).unwrap();
    }

    fn generate_contract_ink_project<A, BA, BN, H, T>() -> InkProject
    where
        A: TypeInfo + 'static,
        BA: TypeInfo + 'static,
        BN: TypeInfo + 'static,
        H: TypeInfo + 'static,
        T: TypeInfo + 'static,
    {
        // let _ = generate_metadata();
        let leaf = LeafLayout::from_key::<u8>(LayoutKey::new(0_u8));
        let layout = Layout::Leaf(leaf);

        #[derive(scale_info::TypeInfo)]
        pub enum NoChainExtension {}

        type ChainExtension = NoChainExtension;
        const MAX_EVENT_TOPICS: usize = 4;
        const BUFFER_SIZE: usize = 1 << 14;

        // given
        let contract: ContractSpec = ContractSpec::new()
            .constructors(vec![ConstructorSpec::from_label("new")
                .selector([94u8, 189u8, 136u8, 214u8])
                .payable(true)
                .args(vec![MessageParamSpec::new("init_value")
                    .of_type(TypeSpec::with_name_segs::<i32, _>(
                        vec!["i32"].into_iter().map(AsRef::as_ref),
                    ))
                    .done()])
                .returns(ReturnTypeSpec::new(None))
                .docs(Vec::new())
                .done()])
            .messages(vec![MessageSpec::from_label("get")
                .selector([37u8, 68u8, 74u8, 254u8])
                .mutates(false)
                .payable(false)
                .args(Vec::new())
                .returns(ReturnTypeSpec::new(TypeSpec::with_name_segs::<i32, _>(
                    vec!["i32"].into_iter().map(AsRef::as_ref),
                )))
                .done()])
            .events(Vec::new())
            .environment(
                EnvironmentSpec::new()
                    .account_id(TypeSpec::with_name_segs::<A, _>(
                        ::core::iter::Iterator::map(
                            ::core::iter::IntoIterator::into_iter(["AccountId"]),
                            ::core::convert::AsRef::as_ref,
                        ),
                    ))
                    .balance(TypeSpec::with_name_segs::<BA, _>(
                        ::core::iter::Iterator::map(
                            ::core::iter::IntoIterator::into_iter(["Balance"]),
                            ::core::convert::AsRef::as_ref,
                        ),
                    ))
                    .hash(TypeSpec::with_name_segs::<H, _>(
                        ::core::iter::Iterator::map(
                            ::core::iter::IntoIterator::into_iter(["Hash"]),
                            ::core::convert::AsRef::as_ref,
                        ),
                    ))
                    .timestamp(TypeSpec::with_name_segs::<T, _>(
                        ::core::iter::Iterator::map(
                            ::core::iter::IntoIterator::into_iter(["Timestamp"]),
                            ::core::convert::AsRef::as_ref,
                        ),
                    ))
                    .block_number(TypeSpec::with_name_segs::<BN, _>(
                        ::core::iter::Iterator::map(
                            ::core::iter::IntoIterator::into_iter(["BlockNumber"]),
                            ::core::convert::AsRef::as_ref,
                        ),
                    ))
                    .chain_extension(TypeSpec::with_name_segs::<ChainExtension, _>(
                        ::core::iter::Iterator::map(
                            ::core::iter::IntoIterator::into_iter(["ChainExtension"]),
                            ::core::convert::AsRef::as_ref,
                        ),
                    ))
                    .max_event_topics(MAX_EVENT_TOPICS)
                    .static_buffer_size(BUFFER_SIZE)
                    .done(),
            )
            .done();

        InkProject::new(layout, contract)
    }

    #[test]
    fn contract_and_node_match() {
        let mut registry = Registry::new();
        registry.register_type(&MetaType::new::<Environment>());

        let portable: PortableRegistry = registry.into();

        let ink_project = generate_contract_ink_project::<
            AccountId,
            Balance,
            BlockNumber,
            Hash,
            Timestamp,
        >();

        let valid = compare_node_env_with_contract(&portable, &ink_project);
        assert!(valid.is_ok(), "{}", valid.err().unwrap())
    }

    #[test]
    fn contract_and_node_mismatch() {
        let mut registry = Registry::new();
        registry.register_type(&MetaType::new::<Environment>());

        let portable: PortableRegistry = registry.into();

        let ink_project =
            generate_contract_ink_project::<AccountId, Balance, BlockNumber, Hash, u8>();

        let result = compare_node_env_with_contract(&portable, &ink_project);
        assert_eq!(
            result.err().unwrap().to_string(),
            "Failed to validate the field: timestamp"
        )
    }
}
