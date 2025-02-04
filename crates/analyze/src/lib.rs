// Copyright (C) Use Ink (UK) Ltd.
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
    bail,
    Result,
};
pub use contract_metadata::Language;

/// Detects the programming language of a smart contract from its PolkaVM
/// binary code.
///
/// This function accepts a binary contract as input and employs a set of heuristics
/// to identify the contract's source language. It currently supports detection of the
/// ink! and Solidity languages.
pub fn determine_language(_code: &[u8]) -> Result<Language> {
    /*
    // todo
    if !start_section && module.custom_sections.keys().any(|e| e == &"producers") {
        return Ok(Language::Solidity)
    } else if start_section
        && module
            .custom_sections
            .keys()
            .any(|e| e == &"sourceMappingURL")
    {
        return Ok(Language::AssemblyScript)
    } else if !start_section
        && (is_ink_function_present(&module)
            || matches!(module.has_function_name("ink_env"), Ok(true)))
    {
        return Ok(Language::Ink)
    }
    */

    bail!("Language unsupported or unrecognized.")
}

#[cfg(test)]
mod tests {
    /*
    // todo

    #[test]
    fn fails_with_unsupported_language() {
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
        let code = &wat::parse_str(contract).expect("Invalid wat.");
        let lang = determine_language(code);
        assert!(lang.is_err());
        assert_eq!(
            lang.unwrap_err().to_string(),
            "Language unsupported or unrecognized."
        );
    }

    #[test]
    fn determines_ink_language() {
        let contract = r#"
        (module
            (type (;0;) (func (param i32 i32 i32)))
            (type (;1;) (func (result i32)))
            (type (;2;) (func (param i32 i32)))
            (import "seal" "foo" (func (;0;) (type 0)))
            (import "seal0" "value_transferred" (func (;1;) (type 2)))
            (import "env" "memory" (memory (;0;) 2 16))
            (func (;2;) (type 2))
            (func (;3;) (type 1) (result i32)
            (local i32 i64 i64)
            global.get 0
            i32.const 32
            i32.sub
            local.tee 0
            global.set 0
            local.get 0
            i64.const 0
            i64.store offset=8
            local.get 0
            i64.const 0
            i64.store
            local.get 0
            i32.const 16
            i32.store offset=28
            local.get 0
            local.get 0
            i32.const 28
            i32.add
            call 1
            local.get 0
            i64.load offset=8
            local.set 1
            local.get 0
            i64.load
            local.set 2
            local.get 0
            i32.const 32
            i32.add
            global.set 0
            i32.const 5
            i32.const 4
            local.get 1
            local.get 2
            i64.or
            i64.eqz
            select
        )
            (global (;0;) (mut i32) (i32.const 65536))
        )"#;
        let code = &wat::parse_str(contract).expect("Invalid wat.");
        let lang = determine_language(code);
        assert!(
            matches!(lang, Ok(Language::Ink)),
            "Failed to detect Ink! language."
        );
    }

    #[test]
    fn determines_solidity_language() {
        let contract = r#"
        (module
            (type (;0;) (func (param i32 i32 i32)))
            (import "env" "memory" (memory (;0;) 16 16))
            (func (;0;) (type 0))
            (@custom "producers" "data")
        )
        "#;
        let code = &wat::parse_str(contract).expect("Invalid wat.");
        let lang = determine_language(code);
        assert!(
            matches!(lang, Ok(Language::Solidity)),
            "Failed to detect Solidity language."
        );
    }

    #[test]
    fn determines_assembly_script_language() {
        let contract = r#"
        (module
            (type $none_=>_none (func))
            (type (;0;) (func (param i32 i32 i32)))
            (import "seal" "foo" (func (;0;) (type 0)))
            (import "env" "memory" (memory $0 2 16))
            (start $~start)
            (func $~start (type $none_=>_none))
            (func (;1;) (type 0))
            (@custom "sourceMappingURL" "data")
        )
        "#;
        let code = &wat::parse_str(contract).expect("Invalid wat.");
        let lang = determine_language(code);
        assert!(
            matches!(lang, Ok(Language::AssemblyScript)),
            "Failed to detect AssemblyScript language."
        );
    }
     */
}
