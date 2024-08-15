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

use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use colored::Colorize;
use wasm_encoder::{
    EntityType,
    ExportSection,
    ImportSection,
    RawSection,
    Section,
};
use wasmparser::{
    ExportSectionReader,
    ImportSectionReader,
    Parser,
    Payload,
};

use anyhow::{
    anyhow,
    Context,
    Error,
    Result,
};

use crate::{
    validate_wasm,
    verbose_eprintln,
    Verbosity,
};

/// Ensures the Wasm memory import of a given module has the maximum number of pages.
///
/// Iterates over the import section, finds the memory import entry if any and adjusts the
/// maximum limit.
fn ensure_maximum_memory_pages(
    imports_reader: &ImportSectionReader,
    maximum_allowed_pages: u64,
) -> Result<ImportSection> {
    let mut memory_found = false;
    let imports = imports_reader.clone().into_iter().try_fold(
        ImportSection::new(), |mut imports, entry| {
            let entry = entry?;
            let mut entity  = EntityType::try_from(
                entry.ty).map_err(|_| anyhow!("Unsupported type in import section"))?;
            if let EntityType::Memory(mut mem) = entity {
                memory_found = true;
               if let Some(requested_maximum) = mem.maximum {
                    // The module already has maximum, check if it is within the limit bail out.
                    if requested_maximum > maximum_allowed_pages {
                        anyhow::bail!(
                            "The wasm module requires {} pages. The maximum allowed number of pages is {}",
                            requested_maximum, maximum_allowed_pages,
                        );
                    }
                }
                else {
                    mem.maximum = Some(maximum_allowed_pages);
                    entity = EntityType::from(mem);
                }
            }
            imports.import(entry.module, entry.name, entity);

            Ok::<_, Error>(imports)
    })?;

    if !memory_found {
        anyhow::bail!(
            "Memory import is not found. Is --import-memory specified in the linker args",
        );
    }
    Ok(imports)
}

/// Strips all custom sections.
///
/// Presently all custom sections are not required so they can be stripped safely.
/// The name section is already stripped by `wasm-opt`.
fn strip_custom_sections(name: &str) -> bool {
    !(name.starts_with("reloc.") || name == "name")
}

/// A contract should export nothing but the "call" and "deploy" functions.
///
/// Any elements not referenced by these exports become orphaned and are removed by
/// `wasm-opt`.
fn strip_export_section(exports_reader: &ExportSectionReader) -> Result<ExportSection> {
    let filtered_exports = exports_reader.clone().into_iter().try_fold(
        ExportSection::new(),
        |mut exports, entry| {
            let entry = entry.context("Parsing of wasm export section failed")?;
            if matches!(entry.kind, wasmparser::ExternalKind::Func)
                && (entry.name == "call" || entry.name == "deploy")
            {
                exports.export(entry.name, entry.kind.into(), entry.index);
            }
            Ok::<_, Error>(exports)
        },
    )?;

    Ok(filtered_exports)
}

/// Load a Wasm file from disk.
pub fn load_module<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
    let path = path.as_ref();
    fs::read(path).context(format!(
        "Loading of wasm module at '{}' failed",
        path.display(),
    ))
}

/// Performs required post-processing steps on the Wasm artifact.
pub fn post_process_wasm(
    optimized_code: &PathBuf,
    skip_wasm_validation: bool,
    verbosity: &Verbosity,
    max_memory_pages: u64,
) -> Result<()> {
    // Deserialize Wasm module from a file.
    let module =
        load_module(optimized_code).context("Loading of optimized wasm failed")?;
    let output =
        post_process_module(&module, skip_wasm_validation, verbosity, max_memory_pages)?;
    fs::write(optimized_code, output)?;
    Ok(())
}

/// Performs required post-processing steps on the Wasm in the buffer.
pub fn post_process_module(
    module: &[u8],
    skip_wasm_validation: bool,
    verbosity: &Verbosity,
    max_memory_pages: u64,
) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    for payload in Parser::new(0).parse_all(module) {
        let payload = payload?;

        match payload {
            Payload::Version { encoding, .. } => {
                output.extend_from_slice(match encoding {
                    wasmparser::Encoding::Component => {
                        anyhow::bail!("Unsupported component section")
                    }
                    wasmparser::Encoding::Module => &wasm_encoder::Module::HEADER,
                });
            }
            Payload::End(_) => break,
            Payload::CustomSection(ref c) => {
                if strip_custom_sections(c.name()) {
                    // Strip custom section
                    continue
                }
            }
            Payload::ExportSection(ref e) => {
                let exports = strip_export_section(e)?;
                exports.append_to(&mut output);
                continue
            }
            Payload::ImportSection(ref i) => {
                let imports = ensure_maximum_memory_pages(i, max_memory_pages)?;
                imports.append_to(&mut output);
                continue
            }
            _ => {}
        }
        // Forward a section without touching it
        if let Some((id, range)) = payload.as_section() {
            RawSection {
                id,
                data: &module[range],
            }
            .append_to(&mut output);
        }
    }

    debug_assert!(
        !output.is_empty(),
        "resulting wasm size of post processing must be > 0"
    );

    if !skip_wasm_validation {
        validate_wasm::validate_import_section(&output)?;
    } else {
        verbose_eprintln!(
            verbosity,
            " {}",
            "Skipping wasm validation! Contract code may be invalid."
                .bright_yellow()
                .bold()
        );
    }

    Ok(output)
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::Verbosity;
    use wasmparser::TypeRef;

    #[test]
    fn post_process_wasm_exceeded_memory_limit() {
        // given
        let contract = r#"
            (module
                (type (;0;) (func (param i32 i32 i32)))
                (import "seal" "foo" (func (;5;) (type 0)))
                (import "env" "memory" (memory (;0;) 2 32))
                (func (;5;) (type 0))
            )"#;
        let module = wabt::wat2wasm(contract).expect("Invalid wabt");

        // when
        let res = post_process_module(&module, true, &Verbosity::Verbose, 16);

        // then
        assert!(res.is_err());
        assert_eq!(
         res.err().unwrap().to_string(),
         "The wasm module requires 32 pages. The maximum allowed number of pages is 16");
    }

    #[test]
    fn post_process_wasm_missing_memory_limit() {
        // given
        let contract = r#"
            (module
                (type (;0;) (func (param i32 i32 i32)))
                (import "seal" "foo" (func (;0;) (type 0)))
                (import "env" "memory" (memory (;0;) 2))
                (func (;1;) (type 0))
            )"#;
        let module = wabt::wat2wasm(contract).expect("Invalid wabt");

        // when
        let output = post_process_module(&module, true, &Verbosity::Verbose, 16)
            .expect("Invalid wasm module");

        // then
        let maximum = Parser::new(0).parse_all(&output).find_map(|p| {
            if let Payload::ImportSection(section) = p.unwrap() {
                section.into_iter().find_map(|e| {
                    if let TypeRef::Memory(mem) = e.unwrap().ty {
                        mem.maximum
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        });
        assert_eq!(maximum, Some(16));
    }

    #[test]
    fn post_process_wasm_missing_memory_import() {
        // given
        let contract = r#"
            (module
                (type (;0;) (func (param i32 i32 i32)))
                (import "seal" "foo" (func (;0;) (type 0)))
                (func (;1;) (type 0))
            )"#;
        let module = wabt::wat2wasm(contract).expect("Invalid wabt");

        // when
        let res = post_process_module(&module, true, &Verbosity::Verbose, 16);

        // then
        assert!(res.is_err());
        assert_eq!(
            res.err().unwrap().to_string(),
            "Memory import is not found. Is --import-memory specified in the linker args"
        );
    }

    #[test]
    fn post_process_wasm_strip_export_section() {
        // given
        let contract = r#"
            (module
                (type (;0;) (func (param i32 i32 i32)))
                (import "seal" "foo" (func (;0;) (type 0)))
                (import "env" "memory" (memory (;0;) 2))
                (func (;1;) (type 0))
                (export "call" (func 1))
                (export "foo" (func 1))
                (export "deploy" (func 1))
                (export "goo" (func 1))
            )"#;
        let module = wabt::wat2wasm(contract).expect("Invalid wabt");

        // when
        let output = post_process_module(&module, true, &Verbosity::Verbose, 1)
            .expect("Invalid wasm module");

        // then
        let exports_count = Parser::new(0).parse_all(&output).find_map(|p| {
            if let Payload::ExportSection(section) = p.unwrap() {
                Some(section.into_iter().count())
            } else {
                None
            }
        });
        assert_eq!(exports_count, Some(2));
    }

    #[test]
    fn post_process_wasm_untouched() {
        // given
        let contract = r#"
            (module
                (type (;0;) (func (param i32 i32 i32)))
                (type (;1;) (func (param i32 i32) (result i32)))
                (type (;2;) (func (param i32 i32)))
                (import "seal" "foo" (func (;0;) (type 0)))
                (import "env" "memory" (memory (;0;) 2 16))
                (func (;1;) (type 0))
                (func (;2;) (type 2))
                (func (;3;) (type 0))
                (export "call" (func 1))
                (export "deploy" (func 1))
                (global (;0;) (mut i32) (i32.const 65536))
                (global (;1;) i32 (i32.const 84291))
                (global (;2;) i32 (i32.const 84304))
                (data (;0;) (i32.const 65536) "test")
            )"#;
        let module = wabt::wat2wasm(contract).expect("Invalid wabt");

        // when
        let output = post_process_module(&module, false, &Verbosity::Verbose, 16)
            .expect("Invalid wasm module");

        // then
        assert_eq!(module, output);
    }
}
