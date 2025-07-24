use colored::Colorize;
use contract_build::{
    verbose_eprintln,
    Verbosity,
};
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

/*
// todo missing comparison for those. not sure if even exposed somewhere.
impl Environment for DefaultEnvironment {
    const MAX_EVENT_TOPICS: usize = 4;
    type ChainExtension = NoChainExtension;
}
 */
fn get_node_env_fields(
    registry: &PortableRegistry,
    _verbosity: &Verbosity, // todo
    path_segments: &Vec<&str>,
) -> Result<Option<Vec<Field<PortableForm>>>> {
    let Some(env_type) = registry.types.iter().find(|t| {
        let len = t.ty.path.segments.len();
        let bound = len.saturating_sub(path_segments.len());
        t.ty.path.segments[bound..].to_vec() == *path_segments
    }) else {
        return Ok(None)
    };

    if let TypeDef::Composite(composite) = &env_type.ty.type_def {
        Ok(Some(composite.fields.clone()))
    } else if let TypeDef::Variant(variant) = &env_type.ty.type_def {
        // todo comment why taking the first is ok
        Ok(variant.variants.first().map(|v| v.fields.clone()))
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
                .first()
                .context("Incorrect format of a field")?
                .ty
                .id;
            return resolve_type_definition(registry, tt_id)
        }
        Ok(tt.type_def.clone())
    } else {
        let param_id = tt
            .type_params
            .first()
            .context("type param is not present")?
            .ty
            .context("concrete type is not present")?
            .id;
        resolve_type_definition(registry, param_id)
    }
}

fn corresponding_type(field_id: &str) -> Result<&'static str> {
    match field_id {
        "free" => Ok("Balance"),
        "owner" => Ok("AccountId"),
        "parent_hash" => Ok("Hash"),
        "now" => Ok("Timestamp"),
        "number" => Ok("BlockNumber"),
        _ => {
            anyhow::bail!(
                "Function `compare` called with unknown field id `{}`",
                field_id
            )
        }
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
    verbosity: &Verbosity,
) -> Result<()> {
    // Compare the field `field_id` in the path of `path_segments` from the
    // `node_registry` with the fitting type from the `ink::Environment`.
    //
    // Errors if comparison unsuccessful.

    // **Does not error if the field is not found in the node metadata! A warning will
    // be printed to `stderr` instead.**
    fn compare_if_possible(
        node_registry: &PortableRegistry,
        contract_metadata: &InkProject,
        verbosity: &Verbosity,
        path_segments: Vec<&str>,
        field_id: &str,
    ) -> Result<()> {
        let Some(env_fields) =
            get_node_env_fields(node_registry, verbosity, &path_segments)?
        else {
            verbose_eprintln!(
                verbosity,
                "{} {}",
                "Warning:".yellow().bold(),
                // todo check website link still works after website revamp
                format!("The chain you are connecting to does not support validating that your environmental contract types are the same as the chain types.\n\
                 We cannot check if the types defined for your contract's `Environment` trait are the same as used on this chain.\
                 See https://use.ink/v6/faq#type-comparison for more info.\n\n\
                 Specifically we failed to find the field `{}::{}` in the chain metadata.\n\
                 This field is compared against your contract's `Environment::{}` type.",
                    path_segments.join("::"),
                    field_id,
                    corresponding_type(field_id)?
                )
                .yellow()
            );
            return Ok(())
        };

        for field in env_fields {
            let field_name = field.name.context("Field does not have a name")?;
            if field_name != field_id {
                continue
            }
            let field_def = resolve_type_definition(node_registry, field.ty.id)?;
            compare_type(
                &path_segments,
                &field_name,
                field_def,
                contract_metadata,
                node_registry,
            )?;
            return Ok(());
        }
        anyhow::bail!(
            "Failed to find field `{}::{}` in node metadata",
            path_segments.join("::"),
            field_id
        )
    }

    // todo should be compared against the `pallet_revive::Currency` config.
    // the `pallet_balances` one is not necessarily used as the `pallet_revive::Currency`.
    compare_if_possible(
        node_registry,
        contract_metadata,
        verbosity,
        vec!["pallet_balances", "types", "AccountData"],
        "free",
    )?;
    compare_if_possible(
        node_registry,
        contract_metadata,
        verbosity,
        // we use `wasm` here, as that is what `pallet-revive` still uses as a name for
        // this module
        vec!["pallet_revive", "wasm", "CodeInfo"],
        "owner",
    )?;
    compare_if_possible(
        node_registry,
        contract_metadata,
        verbosity,
        vec!["sp_runtime", "generic", "header", "Header"],
        "parent_hash",
    )?;
    compare_if_possible(
        node_registry,
        contract_metadata,
        verbosity,
        vec!["sp_runtime", "generic", "header", "Header"],
        "number",
    )?;
    compare_if_possible(
        node_registry,
        contract_metadata,
        verbosity,
        vec!["pallet_timestamp", "pallet", "Call"],
        "now",
    )?;
    Ok(())
}

/// Compares the contract's environment type with a provided type definition.
///
/// Errors if type not found.
fn compare_type(
    path_segments: &Vec<&str>,
    type_name: &str,
    type_def: TypeDef<PortableForm>,
    contract_metadata: &InkProject,
    node_registry: &PortableRegistry,
) -> Result<()> {
    fn bail_with_msg(
        path_segments: &Vec<&str>,
        field_name: &str,
        contract_type: &str,
        node_type: &str,
    ) -> Result<()> {
        let field = format!("{}::{}", path_segments.join("::"), field_name);
        anyhow::bail!("Failed to validate the field `{}`, which must correspond to the contract's `Environment::{}` type.\n\
        Field type in node metadata: {}.\n\
        Field type in contract `Environment` trait: {}", field, corresponding_type(field_name)?,
            node_type,
            contract_type,
        )
    }
    let tt_id = match type_name {
        "free" => contract_metadata.spec().environment().balance().ty().id,
        "owner" => contract_metadata.spec().environment().account_id().ty().id,
        "parent_hash" => contract_metadata.spec().environment().hash().ty().id,
        "now" => contract_metadata.spec().environment().timestamp().ty().id,
        "number" => {
            contract_metadata
                .spec()
                .environment()
                .block_number()
                .ty()
                .id
        }
        _ => anyhow::bail!("Trying to resolve unknown environment type {:?}", type_name),
    };
    let contract_registry = contract_metadata.registry();
    let tt_def = resolve_type_definition(contract_registry, tt_id)?;
    if let TypeDef::Array(node_arr) = &type_def {
        let node_arr_type =
            resolve_type_definition(node_registry, node_arr.type_param.id)?;
        if let TypeDef::Array(contract_arr) = &tt_def {
            if node_arr.len != contract_arr.len {
                anyhow::bail!(
                    "Mismatch in array lengths: {:?} vs {:?}",
                    node_arr,
                    contract_arr
                );
            }
            let contract_arr_type =
                resolve_type_definition(contract_registry, contract_arr.type_param.id)?;
            if contract_arr_type != node_arr_type {
                bail_with_msg(
                    path_segments,
                    type_name,
                    &format!("{contract_arr_type:?}"),
                    &format!("{node_arr_type:?}"),
                )?;
            }
            return Ok(())
        }
    }
    if let TypeDef::Compact(node_compact) = &type_def {
        let node_compact_type =
            resolve_type_definition(node_registry, node_compact.type_param.id)?;
        if tt_def != node_compact_type {
            bail_with_msg(
                path_segments,
                type_name,
                &format!("{tt_def:?}"),
                &format!("{node_compact_type:?}"),
            )?;
        }
        return Ok(())
    }
    if tt_def != type_def {
        bail_with_msg(
            path_segments,
            type_name,
            &format!("{tt_def:?}"),
            &format!("{type_def:?}"),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        compare_node_env_with_contract,
        env_check::resolve_type_definition,
    };
    use contract_build::Verbosity;
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
    use subxt::utils::AccountId32;

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
    #[scale_info(replace_segment("env_check", "pallet_timestamp"))]
    #[scale_info(replace_segment("tests", "pallet"))]
    #[scale_info(replace_segment("PalletTimestamp", "Call"))]
    pub struct PalletTimestamp {
        now: EnvironmentType<u64>,
    }

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    #[scale_info(replace_segment("contract_extrinsics", "sp_runtime"))]
    #[scale_info(replace_segment("env_check", "generic"))]
    #[scale_info(replace_segment("tests", "header"))]
    pub struct Header {
        number: EnvironmentType<u32>,
        // requires any `[u8; 32]`
        parent_hash: EnvironmentType<Hash>,
    }

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    #[scale_info(replace_segment("env_check", "pallet_revive"))]
    // the `wasm` here is because `pallet-revive` still uses that name for their module
    #[scale_info(replace_segment("tests", "wasm"))]
    #[scale_info(replace_segment("PalletRevive", "CodeInfo"))]
    pub struct PalletRevive {
        owner: EnvironmentType<AccountId32>,
    }

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    #[scale_info(replace_segment("env_check", "pallet_balances"))]
    #[scale_info(replace_segment("tests", "types"))]
    #[scale_info(replace_segment("PalletBalances", "AccountData"))]
    pub struct PalletBalances {
        free: EnvironmentType<u128>,
    }

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    pub struct Node {
        pallet_timestamp: PalletTimestamp,
        sp_runtime: Header,
        pallet_revive: PalletRevive,
        pallet_balances: PalletBalances,
    }

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    #[scale_info(replace_segment("tests", "pallet_revive"))]
    pub struct Environment {
        account_id: EnvironmentType<AccountId>,
        balance: EnvironmentType<Balance>,
        hash: EnvironmentType<Hash>,
        hasher: EnvironmentType<Hasher>,
        timestamp: EnvironmentType<Timestamp>,
        block_number: EnvironmentType<BlockNumber>,
    }

    #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
    #[scale_info(replace_segment("tests", "pallet_revive"))]
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
                .returns(ReturnTypeSpec::new(TypeSpec::default()))
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
        registry.register_type(&MetaType::new::<Node>());

        let portable: PortableRegistry = registry.into();

        let ink_project = generate_contract_ink_project::<
            AccountId,
            Balance,
            BlockNumber,
            Hash,
            Timestamp,
        >();

        use std::fs;
        use stdio_override::StderrOverride;
        let tmp_file = tempfile::Builder::new()
            .prefix("cargo-contract.test.")
            .tempfile()
            .expect("temporary file creation failed");
        let guard =
            StderrOverride::from_file(&tmp_file).expect("unable to override stderr");

        let valid =
            compare_node_env_with_contract(&portable, &ink_project, &Verbosity::Default);
        //panic!("{:?}", valid);

        drop(guard);
        //panic!("foo");
        assert!(valid.is_ok(), "{:?}", valid.err());

        let contents = fs::read_to_string(&tmp_file).unwrap();
        assert!(
            !contents.contains("Warning"),
            "still found warning: {contents}"
        );
    }

    #[test]
    fn unable_to_find_corresponding_field_for_timestamp_in_node_metadata() {
        #[derive(Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize)]
        #[scale_info(replace_segment("env_check", "pallet_timestamp"))]
        #[scale_info(replace_segment("tests", "pallet"))]
        #[scale_info(replace_segment("PalletTimestampWithoutNow", "Call"))]
        pub struct PalletTimestampWithoutNow {
            not_now: EnvironmentType<u64>,
        }

        let mut registry = Registry::new();
        registry.register_type(&MetaType::new::<PalletTimestampWithoutNow>());

        let portable: PortableRegistry = registry.into();

        let ink_project =
            generate_contract_ink_project::<AccountId, Balance, BlockNumber, Hash, u8>();

        let result =
            compare_node_env_with_contract(&portable, &ink_project, &Verbosity::Default);
        assert_eq!(
            result.err().unwrap().to_string(),
            "Failed to find field `pallet_timestamp::pallet::Call::now` in node metadata"
        )
    }

    #[test]
    fn contract_and_node_mismatch() {
        let mut registry = Registry::new();
        //registry.register_type(&MetaType::new::<Environment>());
        //registry.register_type(&MetaType::new::<Environment>());
        registry.register_type(&MetaType::new::<PalletTimestamp>());

        let portable: PortableRegistry = registry.into();

        let ink_project =
            generate_contract_ink_project::<AccountId, Balance, BlockNumber, Hash, u8>();

        let result =
            compare_node_env_with_contract(&portable, &ink_project, &Verbosity::Default);
        assert_eq!(
            result.err().unwrap().to_string(),
            "Failed to validate the field `pallet_timestamp::pallet::Call::now`, which must correspond to the contract's `Environment::Timestamp` type.\n\
            Field type in node metadata: Primitive(U64).\n\
            Field type in contract `Environment` trait: Primitive(U8)"
        )
    }
}
