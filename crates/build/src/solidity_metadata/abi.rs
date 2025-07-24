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
    borrow::Cow,
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
use itertools::Itertools;

use crate::CrateMetadata;

/// Generates a Solidity compatible ABI for the ink! smart contract (if possible).
///
/// Ref: <https://docs.soliditylang.org/en/latest/abi-spec.html#abi-json>
pub fn generate_abi(meta: &ink_metadata::sol::ContractMetadata) -> Result<JsonAbi> {
    // Solidity allows only one constructor, we choose the "default" one (or fallback to
    // the first one).
    let ctors = &meta.constructors;
    let ctor = ctors
        .iter()
        .find_or_first(|ctor| ctor.is_default)
        .ok_or_else(|| {
            anyhow::anyhow!("Expected at least one constructor in contract metadata")
        })?;
    if !ctor.is_default && ctors.len() > 1 {
        // Nudge the user to set a default constructor.
        use colored::Colorize;
        eprintln!(
            "{} No default constructor set. \
            \n    A default constructor is necessary to guarantee consistent Solidity compatible \
            metadata output across different `rustc` and `cargo-contract` releases. \
            \n    Learn more at https://use.ink/6.x/macros-attributes/default/",
            "warning:".yellow().bold()
        );
    }
    let ctor_abi = constructor(ctor)?;

    let mut fn_abis: BTreeMap<String, Vec<Function>> = BTreeMap::new();
    for msg in &meta.functions {
        fn_abis
            .entry(msg.name.to_string())
            .or_default()
            .push(message(msg)?);
    }

    let mut event_abis: BTreeMap<String, Vec<Event>> = BTreeMap::new();
    for evt in &meta.events {
        event_abis
            .entry(evt.name.to_string())
            .or_default()
            .push(event(evt)?);
    }

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
    crate_metadata.artifact_directory.join(metadata_file)
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
fn constructor(ctor: &ink_metadata::sol::ConstructorMetadata) -> Result<Constructor> {
    let params = ctor.inputs.iter().map(param_decl).join(",");

    // NOTE: Solidity constructors don't expose a return type.
    let abi_str = format!(
        "constructor({params}){}",
        if ctor.is_payable { " payable" } else { "" }
    );
    Constructor::parse(&abi_str).map_err(|err| {
        anyhow::anyhow!(
            "Failed to parse abi for constructor `{}` : {err}",
            ctor.name
        )
    })
}

/// Returns the function ABI representation for an ink! message.
fn message(msg: &ink_metadata::sol::FunctionMetadata) -> Result<Function> {
    let name = msg.name.as_ref();
    let params = msg.inputs.iter().map(param_decl).join(",");
    let ret_ty = msg.output.as_ref().map(Cow::as_ref);

    let abi_str = format!(
        "function {name}({params}) public{}{}{}",
        // FIXME: (@davidsemakula) ink! does NOT currently enforce it's immutability
        // claims for messages intrinsically (i.e at compile time).
        // Ref: <https://github.com/use-ink/ink/issues/1969>
        if msg.mutates { "" } else { " view" },
        if msg.is_payable { " payable" } else { "" },
        match ret_ty {
            None | Some("()") => String::new(),
            Some(ret_ty) => format!(" returns ({ret_ty})"),
        },
    );
    Function::parse(&abi_str).map_err(|err| {
        anyhow::anyhow!("Failed to parse abi for message `{}` : {err}", msg.name)
    })
}

/// Returns the event ABI representation for an ink! event.
fn event(evt: &ink_metadata::sol::EventMetadata) -> Result<Event> {
    let name = evt.name.as_ref();
    let params = evt
        .params
        .iter()
        .map(|param| {
            let param_name = param.name.as_ref();
            let ty = param.ty.as_ref();
            format!(
                "{ty}{} {param_name}",
                if param.is_topic { " indexed" } else { "" }
            )
        })
        .join(",");

    let abi_str = format!(
        "event {name}({params}){}",
        if evt.is_anonymous { " anonymous" } else { "" }
    );
    Event::parse(&abi_str).map_err(|err| {
        anyhow::anyhow!("Failed to parse abi for event `{}` : {err}", evt.name)
    })
}

/// Returns equivalent Solidity ABI declaration for an ink! constructor or
/// message parameter.
fn param_decl(param: &ink_metadata::sol::ParamMetadata) -> String {
    let name = param.name.as_ref();
    let ty = param.ty.as_ref();
    format!("{ty} {name}")
}
