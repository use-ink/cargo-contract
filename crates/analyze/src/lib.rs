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

use std::{collections::HashMap};
use core::ops::Range;
use anyhow::{
    anyhow, bail, Result
};
pub use contract_metadata::Language;
use parity_wasm::elements::{
    External,
    FuncBody,
    FunctionType,
    ImportSection,
    Instruction,
    Module,
    Type,
    TypeSection,
    ValueType,
};
use wasmparser::{BinaryReaderError, FuncType, Import, Name, NameSectionReader, Operator, Parser, Payload, RefType, TypeRef};

#[derive(Default)]
struct ModuleInfo<'a> {
    custom_sections: HashMap<String, Range<usize>>,
    start_section: Option<(u32, Range<usize>)>,
    module_bytes: &'a [u8],
    // Function idx maps to type idx
    functions: Vec<u32>,
    // Types for inner functions
    types: Vec<FuncType>,
    imports: Vec<Import<'a>>,
    bodies: Vec<Vec<Operator<'a>>>
}

impl <'a> ModuleInfo<'a>{
    fn parse(
        code: &'a [u8],
    ) -> Result<Self> {
        let mut module = Self { custom_sections: HashMap::new(),
            start_section: None,
            module_bytes: code,
            functions: Vec::new(),
            types: Vec::new(),
            imports: Vec::new(),
            bodies: Vec::new(),
        };
        for payload in Parser::new(0).parse_all(module.module_bytes) {
            let payload = payload?;
    
            match payload {
                Payload::Version { encoding, .. } => {
                    match encoding {
                        wasmparser::Encoding::Component => {
                            anyhow::bail!("Unsupported component section")
                        }
                        wasmparser::Encoding::Module => {},
                    };
                }
                Payload::End(_) => break,
                Payload::CustomSection(ref c) => {
                    module.custom_sections.insert(c.name().to_string(), c.range());
                }
                Payload::StartSection {func, range} => {
                    module.start_section = Some((func, range));
                }
                Payload::CodeSectionStart {
                        count: _,
                        range,
                        size: _,
                    } => {
                        let reader = wasmparser::CodeSectionReader::new(&module.module_bytes, range.start)?;

                         for body in reader {
                             let body = body?;
                             let reader = body.get_operators_reader();
                                let operators = reader?;
                                let ops = operators.into_iter().collect::<std::result::Result<Vec<_>, _>>()?;
                                module.bodies.push(ops);
                        }

                    },
                Payload::ImportSection(reader) => {
                    for ty in reader {
                        
                        module.imports.push(ty.clone()?);
                           if let wasmparser::TypeRef::Func(ty) = ty?.ty {
                                // Save imported functions
                                module.functions.push(ty);
                            }
                            
                        
                    }
                }
                Payload::TypeSection(reader) => {
                    // Save function types
                    for ty in reader.into_iter_err_on_gc_types() {
                        module.types.push(ty?);
                    }
                }
                Payload::FunctionSection(reader) => {
                    for ty in reader {
                        module.functions.push(ty?);
                    }
                }
                _ => {}
            }
        }
    
        Ok(module)
    }

    pub fn new(code: &'a[u8]) -> Result<Self> {

        Self::parse(code)

    }

    /// Get custom section names.
    pub fn custom_section_names(&self) -> Vec<String> {
        self.custom_sections.keys().map(ToOwned::to_owned).collect()
    }

    /// Check if functiona name is present in the 'name' custom section
    fn has_function_name(&self, name: &str) -> Result<bool> {
        // The contract compiled in debug mode includes function names in the name section.
        let name_section = self.custom_sections.get("name").ok_or_else(|| anyhow!("Not found"))?;
        let reader = NameSectionReader::new(&self.module_bytes, name_section.start);
        for section in reader {
            if let Name::Function(n) = section? {
                for e in n {
                    let naming = e?;
                    if naming.name == name {
                        return Ok(true)
                    }
                }
            }
        }
        Ok(false)
    }

    pub fn has_start_section(&self) -> bool {
        self.start_section.is_some()
    }

    fn get_function_type_index(&self, function: &FuncType) -> Result<usize> //body
    {
        for (i, ty) in self.types.iter().enumerate() {
            if ty == function {
                return  Ok(i);
            }
        }
        bail!("Not found")
    }

    fn get_function_import_index(&self, name: &str) -> Option<u32> //body
    {
        self.imports.iter().find_map( |e| {
            if e.name == name  {
                if let TypeRef::Func(idx) = e.ty {
                return Some(idx)
                }
            }
            None
        })
    }

    fn filter_function_by_type(
        &self,
        function_type: &FuncType,
    ) -> Result<Vec<Vec<Operator>>> {
        let func_type_index = self.get_function_type_index(function_type)?;
    
        self
            .functions
            .iter()
            .enumerate()
            .filter(|(_, &elem)| elem == func_type_index as u32)
            .map(|(index, _)| {
                self
                    .bodies
                    .get(index)
                    .ok_or(anyhow!("Requested function not found code section")).cloned()
            })
            .collect::<Result<Vec<_>>>()
    }

    /// Checks if a ink! function is present.
fn is_ink_function_present(module: &Module) -> bool {
    let import_section = module
        .import_section()
        .expect("Import setction shall be present");

    // Signature for 'deny_payment' ink! function.
    let ink_func_deny_payment_sig =
        Type::Function(FunctionType::new(vec![], vec![ValueType::I32]));
    // Signature for 'transferred_value' ink! function.
    let ink_func_transferred_value_sig =
        Type::Function(FunctionType::new(vec![ValueType::I32], vec![]));

    // The deny_payment and transferred_value functions internally call the
    // value_transferred function. Getting its index from import section.
    let value_transferred_index =
        // For ink! >=4
        get_function_import_index(import_section, "value_transferred").or(
            // For ink! ^3
            get_function_import_index(import_section, "seal_value_transferred"),
        );

    let mut functions: Vec<&FuncBody> = Vec::new();
    let function_signatures =
        vec![&ink_func_deny_payment_sig, &ink_func_transferred_value_sig];

    for signature in function_signatures {
        if let Ok(mut func) = filter_function_by_type(module, signature) {
            functions.append(&mut func);
        }
    }

    if let Ok(index) = value_transferred_index {
        functions.iter().any(|&body| {
            body.code().elements().iter().any(|instruction| {
                // Matches the 'value_transferred' function.
                matches!(instruction, &Instruction::Call(i) if i as usize == index)
            })
        })
    } else {
        false
    }
}

}

/// Detects the programming language of a smart contract from its WebAssembly (Wasm)
/// binary code.
///
/// This function accepts a Wasm code as input and employs a set of heuristics to identify
/// the contract's source language. It currently supports detection for Ink!, Solidity,
/// and AssemblyScript languages.
pub fn determine_language(code: &[u8]) -> Result<Language> {
    let module = ModuleInfo::new(code)?;

    // let wasm_module: Module = parity_wasm::deserialize_buffer(code)?;
    // let module = wasm_module.clone().parse_names().unwrap_or(wasm_module);
    let start_section = module.has_start_section();

    if !start_section && module.custom_section_names().iter().any(|e| e.as_str() == "producers") {
        return Ok(Language::Solidity)
    } else if start_section && module.custom_section_names().iter().any(|e| e.as_str() == "sourceMappingURL") {
        return Ok(Language::AssemblyScript)
    } else if !start_section
       && /*(is_ink_function_present(&module)*/ matches!(module.has_function_name("ink_env"), Ok(true))    {
        return Ok(Language::Ink)
    }

    bail!("Language unsupported or unrecognized")
}

/// Checks if a ink! function is present.
fn is_ink_function_present(module: &Module) -> bool {
    let import_section = module
        .import_section()
        .expect("Import setction shall be present");

    // Signature for 'deny_payment' ink! function.
    let ink_func_deny_payment_sig =
        Type::Function(FunctionType::new(vec![], vec![ValueType::I32]));
    // Signature for 'transferred_value' ink! function.
    let ink_func_transferred_value_sig =
        Type::Function(FunctionType::new(vec![ValueType::I32], vec![]));

    // The deny_payment and transferred_value functions internally call the
    // value_transferred function. Getting its index from import section.
    let value_transferred_index =
        // For ink! >=4
        get_function_import_index(import_section, "value_transferred").or(
            // For ink! ^3
            get_function_import_index(import_section, "seal_value_transferred"),
        );

    let mut functions: Vec<&FuncBody> = Vec::new();
    let function_signatures =
        vec![&ink_func_deny_payment_sig, &ink_func_transferred_value_sig];

    for signature in function_signatures {
        if let Ok(mut func) = filter_function_by_type(module, signature) {
            functions.append(&mut func);
        }
    }

    if let Ok(index) = value_transferred_index {
        functions.iter().any(|&body| {
            body.code().elements().iter().any(|instruction| {
                // Matches the 'value_transferred' function.
                matches!(instruction, &Instruction::Call(i) if i as usize == index)
            })
        })
    } else {
        false
    }
}

/// Get the function index from the import section.
fn get_function_import_index(
    import_section: &ImportSection,
    field: &str,
) -> Result<usize> {
    import_section
        .entries()
        .iter()
        .filter(|&entry| matches!(entry.external(), External::Function(_)))
        .position(|e| e.field() == field)
        .ok_or(anyhow!("Missing required import for: {}", field))
}

/// Get the function type index from the type section.
fn get_function_type_index(
    type_section: &TypeSection,
    function_type: &Type,
) -> Result<usize> {
    type_section
        .types()
        .iter()
        .position(|e| e == function_type)
        .ok_or(anyhow!("Requested function type not found"))
}

/// Search for a functions in a WebAssembly (Wasm) module that matches a given function
/// type.
///
/// If one or more functions matching the specified type are found, this function returns
/// their bodies in a vector; otherwise, it returns an error.
fn filter_function_by_type<'a>(
    module: &'a Module,
    function_type: &Type,
) -> Result<Vec<&'a FuncBody>> {
    let type_section = module
        .type_section()
        .ok_or(anyhow!("Missing required type section"))?;
    let func_type_index = get_function_type_index(type_section, function_type)?;

    module
        .function_section()
        .ok_or(anyhow!("Missing required function section"))?
        .entries()
        .iter()
        .enumerate()
        .filter(|(_, elem)| elem.type_ref() == func_type_index as u32)
        .map(|(index, _)| {
            module
                .code_section()
                .ok_or(anyhow!("Missing required code section"))?
                .bodies()
                .get(index)
                .ok_or(anyhow!("Requested function not found code section"))
        })
        .collect::<Result<Vec<_>>>()
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
            (import "env" "memory" (memory (;0;) 16 16))
            (func (;0;) (type 0))
        )
        "#;
        let code = wabt::wat2wasm(contract).expect("invalid wabt");
        // Custom sections are not supported in wabt format, injecting using parity_wasm
        let mut module: Module = parity_wasm::deserialize_buffer(&code).unwrap();
        module.set_custom_section("producers".to_string(), Vec::new());
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
            (import "seal" "foo" (func (;0;) (type 0)))
            (import "env" "memory" (memory $0 2 16))
            (start $~start)
            (func $~start (type $none_=>_none))
            (func (;1;) (type 0))
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
