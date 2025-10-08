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
    Result,
    bail,
};
pub use contract_metadata::Language;

/// Detects the programming language of a smart contract from its PolkaVM
/// binary code.
///
/// This function accepts a binary contract as input and employs a set of heuristics
/// to identify the contract's source language. It currently supports detection of the
/// ink! and Solidity languages.
///
/// # Developer Note
///
/// Finding a heuristic to distinguish ink! bytecode vs Solidity bytecode is tricky.
/// This is because the Rust compiler (ink!) compiles to LLVM IR, which is the
/// compiled to RISC-V. The Parity `resolc` compiler compiles Yul to LLVM IR,
/// which is then compiled to RISC-V. So in both cases the IR is already LLVM.
///
/// The heuristic that we have found to work is that _all_ Solidity binaries always
/// have these two imports following each other: `seal_return`, `set_immutable_data`.
/// This is true, even for read-only contracts that never store anything.
///
/// For ink!, we found that _all_ binaries have these two imports right after each
/// other: `seal_return`, `set_storage`. This is also true for read-only contracts.
///
/// Note: It is unclear to us at this moment why both languages compile `set_*`
/// imports into the binary, even if no mutation operations are in the syntax.
pub fn determine_language(code: &[u8]) -> Result<Language> {
    let blob = polkavm_linker::ProgramBlob::parse(code[..].into())
        .expect("cannot parse code blob");
    let mut found_seal_return: bool = false;

    for import in blob.imports().iter().flatten() {
        let import = String::from_utf8_lossy(import.as_bytes());
        if found_seal_return == true && import == "set_storage" {
            return Ok(Language::Ink)
        } else if found_seal_return == true && import == "set_immutable_data" {
            return Ok(Language::Solidity)
        }
        if import == "seal_return" {
            found_seal_return = true;
        }
    }

    bail!("Language unsupported or unrecognized.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determines_solidity_language() {
        for file in std::fs::read_dir("tests/resolc-0.3.0/").unwrap() {
            let path = file.unwrap().path();
            let code = std::fs::read(path).unwrap();
            let lang = determine_language(&code[..]);
            assert!(
                matches!(lang, Ok(Language::Solidity)),
                "Failed to detect Solidity language."
            );
        }
    }

    #[test]
    fn determines_ink_language() {
        for file in std::fs::read_dir("tests/ink-6.0.0-alpha.4/").unwrap() {
            let path = file.unwrap().path();
            let code = std::fs::read(path).unwrap();
            let lang = determine_language(&code[..]);
            assert!(
                matches!(lang, Ok(Language::Ink)),
                "Failed to detect ink! language."
            );
        }
    }
}
