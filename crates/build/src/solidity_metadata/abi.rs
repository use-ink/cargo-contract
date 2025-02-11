// Copyright (C) ink! contributors.
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
    collections::BTreeMap,
    fs::{
        self,
    },
    path::{
        Path,
        PathBuf,
    },
};

use alloy_json_abi::{
    Constructor,
    Event,
    Function,
    JsonAbi,
};
use anyhow::Result;
use ink_metadata::{
    ConstructorSpec,
    EventSpec,
    InkProject,
    MessageParamSpec,
    MessageSpec,
    ReturnTypeSpec,
};
use itertools::Itertools;
use scale_info::{
    form::PortableForm,
    PortableRegistry,
    TypeDef,
    TypeDefPrimitive,
};

use crate::CrateMetadata;

/// Generates a Solidity-compatible ABI for the ink! smart contract (if possible).
///
/// Ref: <https://docs.soliditylang.org/en/latest/abi-spec.html#abi-json>
pub fn generate_abi(ink_project: &InkProject) -> Result<JsonAbi> {
    let registry = ink_project.registry();
    let spec = ink_project.spec();

    // Solidity allows only one constructor, we choose the first one (or fallback to the
    // first one).
    let ctors = spec.constructors();
    let ctor = ctors
        .iter()
        .find_or_first(|ctor| ctor.default())
        .ok_or_else(|| {
            anyhow::anyhow!("Expected at least one constructor in contract metadata")
        })?;
    if !ctor.default() && ctors.len() > 1 {
        // Nudge the user to set a default constructor.
        use colored::Colorize;
        eprintln!(
            "{} No default constructor set. \
            \n    A default constructor is necessary to guarantee consistent Solidity compatible \
            metadata output across different rustc and cargo-contract releases. \
            \n    Learn more at https://use.ink/6.x/macros-attributes/default/",
            "warning:".yellow().bold()
        );
    }
    let ctor_abi = constructor(ctor, registry)?;

    let fn_abis: BTreeMap<_, _> = spec
        .messages()
        .iter()
        .map(|msg| {
            message(msg, registry).map(|fn_abi| (msg.label().clone(), vec![fn_abi]))
        })
        .process_results(|iter| iter.collect())?;

    let event_abis: BTreeMap<_, _> = spec
        .events()
        .iter()
        .map(|event_spec| {
            event(event_spec, registry)
                .map(|event_abi| (event_spec.label().clone(), vec![event_abi]))
        })
        .process_results(|iter| iter.collect())?;

    Ok(JsonAbi {
        constructor: Some(ctor_abi),
        fallback: None,
        receive: None,
        functions: fn_abis,
        events: event_abis,
        errors: BTreeMap::new(),
    })
}

/// Get the path of the Solidity compatible contract ABI file.
pub fn abi_path(crate_metadata: &CrateMetadata) -> PathBuf {
    let metadata_file = format!("{}.abi", crate_metadata.contract_artifact_name);
    crate_metadata.target_directory.join(metadata_file)
}

/// Writes a Solidity compatible ABI file.
///
/// Ref: <https://docs.soliditylang.org/en/latest/abi-spec.html#abi-json>
pub fn write_abi<P>(abi: &JsonAbi, path: P) -> Result<()>
where
    P: AsRef<Path>,
{
    let json = serde_json::to_string(abi)?;
    fs::write(path, json)?;

    Ok(())
}

/// Returns the constructor ABI representation for an ink! constructor.
fn constructor(
    ctor: &ConstructorSpec<PortableForm>,
    registry: &PortableRegistry,
) -> Result<Constructor> {
    let params = ctor
        .args()
        .iter()
        .map(|param| {
            param_decl(param, registry, &format!("constructor `{}`", ctor.label()))
        })
        .process_results(|mut iter| iter.join(","))?;

    // NOTE: Solidity constructors don't expose a return type.
    let abi_str = format!(
        "constructor({params}){}",
        if ctor.payable { " payable" } else { "" }
    );
    Constructor::parse(&abi_str).map_err(|err| {
        anyhow::anyhow!(
            "Failed to parse abi for constructor `{}` : {err}",
            ctor.label()
        )
    })
}

/// Returns the function ABI representation for an ink! message.
fn message(
    msg: &MessageSpec<PortableForm>,
    registry: &PortableRegistry,
) -> Result<Function> {
    let name = msg.label();
    let params = msg
        .args()
        .iter()
        .map(|param| param_decl(param, registry, &format!("message `{}`", name)))
        .process_results(|mut iter| iter.join(","))?;
    let ret_ty = return_ty(msg.return_type(), registry, &format!("message `{}`", name))?;

    let abi_str = format!(
        "function {name}({params}) public{}{}{}",
        // FIXME: (@davidsemakula) ink! does NOT currently enforce it's immutability
        // claims for messages intrinsically (i.e at compile time).
        // Ref: <https://github.com/use-ink/ink/issues/1969>
        if msg.mutates() { "" } else { " view" },
        if msg.payable() { " payable" } else { "" },
        if ret_ty.is_empty() || ret_ty == "()" {
            String::new()
        } else {
            format!(" returns ({ret_ty})")
        },
    );
    Function::parse(&abi_str).map_err(|err| {
        anyhow::anyhow!("Failed to parse abi for message `{}` : {err}", msg.label())
    })
}

/// Returns the event ABI representation for an ink! event.
fn event(
    event_spec: &EventSpec<PortableForm>,
    registry: &PortableRegistry,
) -> Result<Event> {
    let name = event_spec.label();
    let params = event_spec
        .args()
        .iter()
        .map(|param| {
            let param_name = param.label();
            let ty_id = param.ty().ty().id;
            let sol_ty = resolve_ty(
                ty_id,
                registry,
                &format!("arg `{param_name}` for event `{name}`"),
            );
            // TODO: (@davidsemakula) should we simply omit events with Solidity ABI
            // incompatible types instead of bailing?
            sol_ty.map(|ty| {
                format!(
                    "{ty}{} {param_name}",
                    if param.indexed() { " indexed" } else { "" }
                )
            })
        })
        .process_results(|mut iter| iter.join(","))?;

    let abi_str = format!(
        "event {name}({params}){}",
        if event_spec.signature_topic().is_none() {
            " anonymous"
        } else {
            ""
        }
    );
    Event::parse(&abi_str).map_err(|err| {
        anyhow::anyhow!(
            "Failed to parse abi for event `{}` : {err}",
            event_spec.label()
        )
    })
}

/// Returns equivalent Solidity ABI declaration (if any) for an ink! constructor or
/// message parameter.
fn param_decl(
    param: &MessageParamSpec<PortableForm>,
    registry: &PortableRegistry,
    msg: &str,
) -> Result<String> {
    let name = param.label();
    let ty_id = param.ty().ty().id;
    let sol_ty = resolve_ty(ty_id, registry, &format!("arg `{name}` for {}", msg));
    sol_ty.map(|ty| format!("{ty} {name}"))
}

// Returns the "user-defined" return type for an ink! message.
//
// **NOTE:** The return type for ink! messages is `Result<T, ink::LangError>`, however,
// the ABI return type we're interested in is the "user-defined" `T` type.
fn return_ty(
    ret_ty: &ReturnTypeSpec<PortableForm>,
    registry: &PortableRegistry,
    msg: &str,
) -> Result<String> {
    let id = ret_ty.ret_type().ty().id;
    let ty = registry
        .resolve(id)
        .unwrap_or_else(|| panic!("Failed to resolve return type `#{}` in {}", id, msg));
    if let TypeDef::Variant(type_def_variant) = &ty.type_def {
        let ok_field = type_def_variant.variants.first().and_then(|v| {
            (v.name == "Ok" && v.fields.len() == 1).then_some(&v.fields[0])
        });
        if let Some(field) = ok_field {
            return resolve_ty(field.ty.id, registry, &format!("return type for {msg}"));
        }
    }

    anyhow::bail!(
        "Expected `Result<T, ink::LangError>` return type for {}",
        msg
    )
}

/// Convenience macro for emitting errors for ink! types that are NOT compatible with any
/// Solidity ABI type.
macro_rules! incompatible_ty {
    ($msg: expr, $ty_def: expr) => {
        anyhow::bail!("Solidity ABI incompatible type in {}: {:?}", $msg, $ty_def)
    };
}

/// Returns the equivalent Solidity ABI type (if any) for an ink! type (represented by the
/// given id in ink! project metadata).
///
/// Ref: <https://docs.soliditylang.org/en/latest/abi-spec.html#mapping-solidity-to-abi-types>
pub fn resolve_ty(id: u32, registry: &PortableRegistry, msg: &str) -> Result<String> {
    let ty = registry
        .resolve(id)
        .unwrap_or_else(|| panic!("Failed to resolve type `#{}` in {}", id, msg));
    match &ty.type_def {
        TypeDef::Composite(_) => {
            let path_segments: Vec<_> =
                ty.path.segments.iter().map(String::as_str).collect();
            let ty = match path_segments.as_slice() {
                ["ink_primitives", "types", "AccountId"]
                | ["ink_primitives", "types", "Hash"]
                | ["primitive_types", "H256"] => "bytes32",
                ["ink_primitives", "types", "Address"] => "address",
                ["primitive_types", "H160"] => "bytes20",
                ["primitive_types", "U256"] => "uint256",
                // NOTE: `bytes1` sequences and arrays are "normalized" to `bytes` or
                // `bytes<N>` at wrapping `TypeDef::Sequence` or
                // `TypeDef::Array` match arm (if appropriate).
                ["ink_primitives", "types", "Byte"] => "bytes1",
                _ => incompatible_ty!(msg, ty),
            };
            Ok(ty.to_string())
        }
        TypeDef::Variant(type_def_variant) => {
            // Unit-only enums (i.e. enums that contain only unit variants) are
            // represented as uint8.
            // Ref: <https://docs.soliditylang.org/en/latest/abi-spec.html#mapping-solidity-to-abi-types>
            // NOTE: This actually checks if an enum is field-less, however, field-less
            // and unit-only enums have an identical representation in ink! metadata.
            // Ref: <https://doc.rust-lang.org/reference/items/enumerations.html#r-items.enum.fieldless>
            // Ref: <https://doc.rust-lang.org/reference/items/enumerations.html#r-items.enum.unit-only>
            let contains_fields = type_def_variant
                .variants
                .iter()
                .any(|variant| !variant.fields.is_empty());
            if !contains_fields {
                Ok("uint8".to_string())
            } else {
                incompatible_ty!(msg, ty)
            }
        }
        TypeDef::Sequence(type_def_seq) => {
            let elem_ty_id = type_def_seq.type_param.id;
            let elem_ty = resolve_ty(elem_ty_id, registry, msg)?;
            let normalized_ty = if elem_ty == "bytes1" {
                // Normalize `bytes1[]` to `bytes`.
                // Ref: <https://docs.soliditylang.org/en/latest/types.html#bytes-and-string-as-arrays>
                "bytes".to_string()
            } else {
                format!("{elem_ty}[]")
            };
            Ok(normalized_ty)
        }
        TypeDef::Array(type_def_array) => {
            let elem_ty_id = type_def_array.type_param.id;
            let elem_ty = resolve_ty(elem_ty_id, registry, msg)?;
            let len = type_def_array.len;
            let normalized_ty = if elem_ty == "bytes1" && (1..=32).contains(&len) {
                // Normalize `bytes1[N]` to `bytes<N>` for `1 <= N <= 32`.
                // Ref: <https://docs.soliditylang.org/en/latest/types.html#fixed-size-byte-arrays>
                format!("bytes{len}")
            } else {
                format!("{elem_ty}[{len}]")
            };
            Ok(normalized_ty)
        }
        TypeDef::Tuple(type_def_tuple) => {
            let tys = type_def_tuple
                .fields
                .iter()
                .map(|field| resolve_ty(field.id, registry, msg))
                .process_results(|mut iter| iter.join(","))?;
            Ok(format!("({tys})"))
        }
        TypeDef::Primitive(type_def_primitive) => {
            primitive_ty(type_def_primitive, msg).map(ToString::to_string)
        }
        TypeDef::Compact(_) | TypeDef::BitSequence(_) => {
            incompatible_ty!(msg, ty)
        }
    }
}

/// Returns the equivalent Solidity elementary type (if any) for an ink! primitive type
/// (represented by the given id in ink! project metadata).
///
/// Ref: <https://docs.soliditylang.org/en/latest/abi-spec.html#mapping-solidity-to-abi-types>
fn primitive_ty(ty_def: &TypeDefPrimitive, msg: &str) -> Result<&'static str> {
    let sol_ty = match ty_def {
        TypeDefPrimitive::Bool => "bool",
        // TODO: (@davidsemakula) can we represent char as a `bytes4` fixed-size
        // array and interpret it in overlong encoding?
        // Ref: <https://en.wikipedia.org/wiki/UTF-8#overlong_encodings>
        TypeDefPrimitive::Char => {
            incompatible_ty!(msg, ty_def);
        }
        // NOTE: Rust strings are UTF-8, while solidity string literals
        // only support ASCII characters, but Solidity also has unicode literals.
        // However, the Solidity ABI spec uses `string` for both, and claims that `string`
        // is a "dynamic sized unicode string assumed to be UTF-8 encoded", so presumably
        // this is fine.
        // Ref: <https://docs.soliditylang.org/en/latest/abi-spec.html#types>
        // Ref: <https://docs.soliditylang.org/en/latest/types.html#string-literals-and-types>
        // Ref: <https://docs.soliditylang.org/en/latest/types.html#unicode-literals>
        TypeDefPrimitive::Str => "string",
        TypeDefPrimitive::U8 => "uint8",
        TypeDefPrimitive::U16 => "uint16",
        TypeDefPrimitive::U32 => "uint32",
        TypeDefPrimitive::U64 => "uint64",
        TypeDefPrimitive::U128 => "uint128",
        TypeDefPrimitive::U256 => "uint256",
        TypeDefPrimitive::I8 => "int8",
        TypeDefPrimitive::I16 => "int16",
        TypeDefPrimitive::I32 => "int32",
        TypeDefPrimitive::I64 => "int64",
        TypeDefPrimitive::I128 => "int128",
        TypeDefPrimitive::I256 => "int256",
    };
    Ok(sol_ty)
}
