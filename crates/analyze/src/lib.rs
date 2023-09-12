// Copyright 2018-2023 Parity Technologies (UK) Ltd.
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
#![deny(unused_crate_dependencies)]
use anyhow::{
    anyhow,
    bail,
    Result,
};
pub use contract_metadata::Language;
use parity_wasm::elements::Module;

/// Detects the programming language of a smart contract from its WebAssembly (Wasm)
/// binary code.
///
/// This function accepts a Wasm code as input and employs a set of heuristics to identify
/// the contract's source language. It currently supports detection for Ink!, Solidity,
/// and AssemblyScript languages.
///
/// If multiple language patterns are found in the code, the function returns an error.
pub fn determine_language(code: &[u8]) -> Result<Language> {
    let module: Module = parity_wasm::deserialize_buffer(code)?;
    let import_section = module.import_section();
    let start_section = module.start_section();
    let mut custom_sections = module.custom_sections().map(|e| e.name()).peekable();

    let import_section_first = import_section
        .ok_or(anyhow!("Missing required import section"))?
        .entries()
        .first()
        .map(|e| e.field())
        .ok_or(anyhow!("Missing required import section"))?;

    if import_section_first != "memory"
        && start_section.is_none()
        && custom_sections.peek().is_none()
    {
        return Ok(Language::Ink)
    } else if import_section_first == "memory"
        && start_section.is_none()
        && custom_sections.any(|e| e == "name")
    {
        return Ok(Language::Solidity)
    } else if import_section_first != "memory"
        && start_section.is_some()
        && custom_sections.any(|e| e == "sourceMappingURL")
    {
        return Ok(Language::AssemblyScript)
    }

    bail!("Language unsupported or unrecognized")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failes_with_unsupported_language() {
        let contract = r#"
        (module
            (type $none_=>_none (func))
            (type (;0;) (func (param i32 i32 i32)))
            (import "env" "memory" (func (;5;) (type 0)))
            (start $~start)
            (func $~start (type $none_=>_none))
            (func (;5;) (type 0))
        )
        "#;
        let code = wabt::wat2wasm(contract).expect("invalid wabt");
        let lang = determine_language(&code);
        assert!(lang.is_err());
        assert_eq!(
            lang.unwrap_err().to_string(),
            "Language unsupported or unrecognized"
        );
    }

    #[test]
    fn determines_ink_language() {
        let contract = r#"
        (module
            (type (;0;) (func (param i32 i32 i32)))
            (import "seal" "foo" (func (;5;) (type 0)))
            (import "env" "memory" (func (;5;) (type 0)))
            (func (;5;) (type 0))
        )"#;
        let code = wabt::wat2wasm(contract).expect("invalid wabt");
        let lang = determine_language(&code);
        assert!(
            matches!(lang, Ok(Language::Ink)),
            "Failed to detect Ink! language"
        );
    }

    #[test]
    fn determines_solidity_language() {
        let contract = r#"
        (module
            (type (;0;) (func (param i32 i32 i32)))
            (import "env" "memory" (func (;5;) (type 0)))
            (func (;5;) (type 0))
        )
        "#;
        let code = wabt::wat2wasm(contract).expect("invalid wabt");
        // Custom sections are not supported in wabt format, injecting using parity_wasm
        let mut module: Module = parity_wasm::deserialize_buffer(&code).unwrap();
        module.set_custom_section("name".to_string(), Vec::new());
        let code = module.into_bytes().unwrap();
        let lang = determine_language(&code);
        assert!(
            matches!(lang, Ok(Language::Solidity)),
            "Failed to detect Solidity language"
        );
    }

    #[test]
    fn determines_assembly_script_language() {
        let contract = r#"
        (module
            (type $none_=>_none (func))
            (type (;0;) (func (param i32 i32 i32)))
            (import "seal" "foo" (func (;5;) (type 0)))
            (import "env" "memory" (func (;5;) (type 0)))
            (start $~start)
            (func $~start (type $none_=>_none))
            (func (;5;) (type 0))
        )
        "#;
        let code = wabt::wat2wasm(contract).expect("invalid wabt");
        // Custom sections are not supported in wabt format, injecting using parity_wasm
        let mut module: Module = parity_wasm::deserialize_buffer(&code).unwrap();
        module.set_custom_section("sourceMappingURL".to_string(), Vec::new());
        let code = module.into_bytes().unwrap();
        let lang = determine_language(&code);
        assert!(
            matches!(lang, Ok(Language::AssemblyScript)),
            "Failed to detect AssemblyScript language"
        );
    }
}
