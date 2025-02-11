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

use std::collections::HashMap;

use anyhow::Result;
use contract_metadata::Contract;
use ink_metadata::{
    EventSpec,
    InkProject,
    MessageSpec,
};
use itertools::Itertools;
use scale_info::{
    form::PortableForm,
    PortableRegistry,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::abi;

/// NatSpec developer documentation of the contract.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DevDoc {
    /// The version of the NatSpec format.
    pub version: u8,
    /// Kind of NatSpec documentation (i.e. "dev").
    pub kind: NatSpecKind,
    /// Author of the contract.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Describes the contract/interface.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Extra details for developers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    /// Storage developer documentation, keys are storage keys.
    #[serde(rename = "stateVariables")]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub state_variables: HashMap<String, ItemDevDoc>,
    /// Function developer documentation, keys are canonical function signatures.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub methods: HashMap<String, ItemDevDoc>,
    /// Events developer documentation, keys are canonical event signatures.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub events: HashMap<String, ItemDevDoc>,
    /// Errors developer documentation, keys are canonical error signatures.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub errors: HashMap<String, ItemDevDoc>,
}

/// NatSpec user documentation of the contract.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UserDoc {
    /// The version of the NatSpec format.
    pub version: u8,
    /// Kind of NatSpec documentation (i.e. "user").
    pub kind: NatSpecKind,
    /// Description for an end-user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
    /// Function user documentation, keys are canonical function signatures.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub methods: HashMap<String, ItemUserDoc>,
    /// Events user documentation, keys are canonical event signatures.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub events: HashMap<String, ItemUserDoc>,
    /// Errors user documentation, keys are canonical error signatures.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub errors: HashMap<String, ItemUserDoc>,
}

/// Kind of NatSpec documentation (i.e. developer or user).
///
/// Ref: <https://docs.soliditylang.org/en/latest/natspec-format.html>
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NatSpecKind {
    /// Developer-focused documentation.
    Dev,
    /// End-user-facing documentation.
    User,
}

/// NatSpec item description for a developer.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ItemDevDoc {
    /// Description for a developer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    /// Item parameter descriptions, keys are parameter names.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub params: HashMap<String, String>,
    /// Item return type descriptions (if any).
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub returns: HashMap<String, String>,
}

impl ItemDevDoc {
    /// Creates a details-only developer documentation item.
    fn details(docs: String) -> Self {
        Self {
            details: Some(docs),
            params: HashMap::new(),
            returns: HashMap::new(),
        }
    }

    /// Creates a details and params only developer documentation item (e.g. for events).
    fn details_and_params(docs: String, params: HashMap<String, String>) -> Self {
        Self {
            details: Some(docs),
            params,
            returns: HashMap::new(),
        }
    }
}

/// NatSpec item description for an end-user.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ItemUserDoc {
    /// Description for an end-user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

/// Generates a Solidity-compatible ABI for the ink! smart contract (if possible).
///
/// Ref: <https://docs.soliditylang.org/en/latest/natspec-format.html>
pub fn generate_natspec(
    ink_project: &InkProject,
    contract: Contract,
) -> Result<(DevDoc, UserDoc)> {
    let registry = ink_project.registry();
    let spec = ink_project.spec();

    let method_docs: HashMap<_, _> = spec
        .messages()
        .iter()
        .filter_map(|msg| message(msg, registry))
        .collect();
    let event_docs: HashMap<_, _> = spec
        .events()
        .iter()
        .filter_map(|event_spec| event(event_spec, registry))
        .collect();

    let dev_doc = DevDoc {
        version: 1,
        kind: NatSpecKind::Dev,
        author: concat_non_empty(&contract.authors, ", "),
        title: contract.description.clone(),
        details: concat_non_empty(spec.docs(), "\n"),
        // FIXME: (@davidsemakula) add storage documentation.
        state_variables: HashMap::new(),
        methods: method_docs,
        events: event_docs,
        // TODO: (@davidsemakula) add errors documentation?.
        errors: HashMap::new(),
    };
    let user_doc = UserDoc {
        version: 1,
        kind: NatSpecKind::User,
        notice: contract.description,
        // NOTE: We assume ink!/Rust doc comments are developer docs, so we have no way of
        // representing the equivalent of NatSpec user docs at the moment.
        methods: HashMap::new(),
        events: HashMap::new(),
        errors: HashMap::new(),
    };

    Ok((dev_doc, user_doc))
}

/// Returns the function signature and developer documentation (if any).
fn message(
    msg: &MessageSpec<PortableForm>,
    registry: &PortableRegistry,
) -> Option<(String, ItemDevDoc)> {
    let name = msg.label();

    // Bails if message has no docs.
    let docs = concat_non_empty(msg.docs(), "\n")?;

    // Generates the function's canonical signature.
    // NOTE: Bails if any parameter has a Solidity ABI incompatible type.
    // NOTE: Rust doesn't currently support doc comments (i.e. rustdoc) for function
    // parameters.
    // Ref: <https://doc.rust-lang.org/reference/items/functions.html#attributes-on-function-parameters>
    let param_tys = msg
        .args()
        .iter()
        .map(|param| {
            let param_name = param.label();
            let ty_id = param.ty().ty().id;
            abi::resolve_ty(
                ty_id,
                registry,
                &format!("arg `{param_name}` for message `{}`", name),
            )
        })
        .process_results(|mut iter| iter.join(","))
        .ok()?;
    let fn_sig = format!("{name}({param_tys})");

    Some((fn_sig, ItemDevDoc::details(docs)))
}

/// Returns the function signature and developer documentation (if any).
fn event(
    event_spec: &EventSpec<PortableForm>,
    registry: &PortableRegistry,
) -> Option<(String, ItemDevDoc)> {
    let name = event_spec.label();

    // Bails if event has no docs.
    let docs = concat_non_empty(event_spec.docs(), "\n")?;

    // Generates the event's canonical signature and param docs.
    // NOTE: Bails if any parameter has a Solidity ABI incompatible type.
    let mut param_tys = Vec::new();
    let mut param_docs = HashMap::new();
    for param in event_spec.args() {
        let param_name = param.label();
        let ty_id = param.ty().ty().id;

        let ty = abi::resolve_ty(
            ty_id,
            registry,
            &format!("arg `{param_name}` for event `{}`", name),
        )
        .ok()?;
        param_tys.push(ty);
        if let Some(docs) = concat_non_empty(param.docs(), "\n") {
            param_docs.insert(param_name.to_string(), docs);
        }
    }
    let event_sig = format!("{name}({})", param_tys.join(","));

    Some((event_sig, ItemDevDoc::details_and_params(docs, param_docs)))
}

/// Given a slice of strings, returns a non-empty doc string that's a concatenation of
/// all the non-empty input strings.
fn concat_non_empty(input: &[String], sep: &str) -> Option<String> {
    (!input.is_empty()).then_some(input.join(sep))
}
