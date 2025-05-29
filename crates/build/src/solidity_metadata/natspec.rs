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
use itertools::Itertools;
use serde::{
    Deserialize,
    Serialize,
};

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
    meta: &ink_metadata::sol::ContractMetadata,
    contract: Contract,
) -> Result<(DevDoc, UserDoc)> {
    let method_docs: HashMap<_, _> = meta.functions.iter().filter_map(message).collect();
    let event_docs: HashMap<_, _> = meta.events.iter().filter_map(event).collect();

    let dev_doc = DevDoc {
        version: 1,
        kind: NatSpecKind::Dev,
        author: concat_non_empty(&contract.authors, ", "),
        title: contract.description.clone(),
        details: (!meta.docs.is_empty()).then_some(meta.docs.to_string()),
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
fn message(msg: &ink_metadata::sol::FunctionMetadata) -> Option<(String, ItemDevDoc)> {
    let name = msg.name.as_ref();
    let docs = msg.docs.to_string();

    // Generates the function's canonical signature.
    // NOTE: Rust doesn't currently support doc comments (i.e. rustdoc) for function
    // parameters.
    // Ref: <https://doc.rust-lang.org/reference/items/functions.html#attributes-on-function-parameters>
    let param_tys = msg.inputs.iter().map(|param| param.ty.as_ref()).join(",");
    let fn_sig = format!("{name}({param_tys})");

    Some((fn_sig, ItemDevDoc::details(docs)))
}

/// Returns the function signature and developer documentation (if any).
fn event(evt: &ink_metadata::sol::EventMetadata) -> Option<(String, ItemDevDoc)> {
    let name = evt.name.as_ref();
    let docs = evt.docs.to_string();

    // Generates the event's canonical signature and param docs.
    let mut param_tys = Vec::new();
    let mut param_docs = HashMap::new();
    for param in &evt.params {
        param_tys.push(param.ty.as_ref());
        if !param.docs.is_empty() {
            param_docs.insert(param.name.to_string(), param.docs.to_string());
        }
    }

    let sig = format!("{name}({})", param_tys.join(","));
    Some((sig, ItemDevDoc::details_and_params(docs, param_docs)))
}

/// Given a slice of strings, returns a non-empty doc string that's a concatenation of
/// all the non-empty input strings.
fn concat_non_empty(input: &[String], sep: &str) -> Option<String> {
    (!input.is_empty()).then_some(input.join(sep))
}
