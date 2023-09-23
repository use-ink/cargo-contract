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
use parity_wasm::elements::{
    FuncBody,
    FunctionType,
    Module,
    Type,
    ValueType,
};

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

    // Looking for the ink! function signature: pub fn deny_payment<E>() -> Result<(),
    // DispatchError> While the function is declared with the inline attribute, the
    // compiler does not appear to inline it, even when building in release mode.
    // (type (;4;) (func (result i32)))
    // (func (;7;) (type 4) (result i32)
    let ink_func_sig = Type::Function(FunctionType::new(vec![], vec![ValueType::I32]));

    let import_section_first = import_section
        .ok_or(anyhow!("Missing required import section"))?
        .entries()
        .first()
        .map(|e| e.field())
        .ok_or(anyhow!("Missing required import section"))?;

    if import_section_first != "memory"
        && start_section.is_none()
        && custom_sections.peek().is_none()
        && find_function(&module, &ink_func_sig).is_ok()
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

/// Search for a functions in a WebAssembly (Wasm) module that matches a given function
/// type.
///
/// If one or more functions matching the specified type are found, this function returns
/// their bodies in a vector; otherwise, it returns an error.
fn find_function<'a>(
    module: &'a Module,
    function_type: &Type,
) -> Result<Vec<&'a FuncBody>> {
    let func_type_idx = module
        .type_section()
        .ok_or(anyhow!("Missing required type section"))?
        .types()
        .iter()
        .position(|e| e == function_type)
        .ok_or(anyhow!("Requested function type not found"))?;

    let functions = module
        .function_section()
        .ok_or(anyhow!("Missing required function section"))?
        .entries()
        .iter()
        .enumerate()
        .filter(|(_, elem)| elem.type_ref() == func_type_idx as u32)
        .map(|(idx, _)| {
            module
                .code_section()
                .ok_or(anyhow!("Missing required code section"))?
                .bodies()
                .get(idx)
                .ok_or(anyhow!("Requested function not found code section"))
        })
        .collect::<Result<Vec<_>>>()?;

    if functions.is_empty() {
        bail!("Function not found");
    }
    Ok(functions)
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
            (type (;1;) (func (result i32)))
            (import "seal" "foo" (func (;5;) (type 0)))
            (import "env" "memory" (func (;5;) (type 0)))
            (func (;5;) (type 0))
            (func (;6;) (type 1) (result i32)
            (local i32 i64 i64)
            local.get 0)
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
