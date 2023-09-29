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

use anyhow::{
    anyhow,
    bail,
    Result,
};
use semver::{
    Version,
    VersionReq,
};

/// Version of the currently executing `cargo-contract` binary.
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, serde::Deserialize)]
struct CompatibilityList {
    #[serde(rename = "cargo-contract")]
    cargo_contracts: Vec<CargoContract>,
}

#[derive(Debug, serde::Deserialize)]
struct CargoContract {
    version: Version,
    #[serde(rename = "ink-versions")]
    ink_req: Vec<VersionReq>,
}

/// Checks whether the contract's ink! version is compatible with the cargo-contract
/// binary.
pub fn check_contract_ink_compatibility(ink_version: &Version) -> Result<()> {
    let compatibility = include_str!("../compatibility_list.json");
    let compatibility_list: CompatibilityList = serde_json::from_str(compatibility)?;
    let cargo_contract_version =
        semver::Version::parse(VERSION).expect("Parsing version failed");

    // Check for compatibility
    let compatible = compatibility_list.cargo_contracts.iter().find(|&e| {
        e.version == cargo_contract_version
            && e.ink_req.iter().any(|req| req.matches(ink_version))
    });

    if compatible.is_none() {
        // Find matching ink! versions
        let ink_matches = compatibility_list
            .cargo_contracts
            .iter()
            .find_map(|e| {
                if e.version == cargo_contract_version {
                    return Some(&e.ink_req)
                }
                None
            })
            .map(|req_list| {
                let versions = req_list
                    .iter()
                    .map(|req| format!("'{}'", req))
                    .collect::<Vec<_>>()
                    .join(" or ");
                format!("update the contract ink! to version {}", versions)
            })
            .unwrap_or_default();

        // Find best cargo-contract version
        let cargo_contract_match = compatibility_list
            .cargo_contracts
            .iter()
            .filter_map(|e| {
                if e.ink_req.iter().any(|req| req.matches(ink_version)) {
                    return Some(&e.version)
                }
                None
            })
            .max_by(|a, b| {
                match (!a.pre.is_empty(), !b.pre.is_empty()) {
                    (false, true) => std::cmp::Ordering::Greater,
                    (true, false) => std::cmp::Ordering::Less,
                    (_, _) => a.cmp(b),
                }
            })
            .map(|ver| format!("update the cargo-contract to version '{}'", ver))
            .unwrap_or_default();

        let matches = [cargo_contract_match, ink_matches].join(" or ");
        if !matches.is_empty() {
            return Err(anyhow!(
                "The cargo-contract is not compatible with the contract's ink! version. Please {matches}"
            ))
        } else {
            bail!("The cargo-contract is not compatible with the contract's ink! version, but a matching version could not be found")
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn ink_check_failes_when_incompatible_version() {
        let ink_version = Version::new(3, 2, 0);
        let res = check_contract_ink_compatibility(&ink_version)
            .expect_err("Ink version check should fail");

        assert_eq!(
            res.to_string(),
            "The cargo-contract is not compatible with the contract's ink! version. Please update the cargo-contract to version '1.5.0' or update the contract ink! to version '^4.0.0-alpha.3' or '^4.0.0'"
        );

        let ink_version =
            Version::parse("4.0.0-alpha.1").expect("Parsing version must work");
        let res = check_contract_ink_compatibility(&ink_version)
            .expect_err("Ink version check should fail");

        assert_eq!(
            res.to_string(),
            "The cargo-contract is not compatible with the contract's ink! version. Please update the cargo-contract to version '1.5.0' or update the contract ink! to version '^4.0.0-alpha.3' or '^4.0.0'"
        );
    }

    #[test]
    fn ink_check_succeeds_when_compatible_version() {
        let ink_version = Version::new(4, 2, 3);
        let res = check_contract_ink_compatibility(&ink_version);
        assert!(res.is_ok());

        let ink_version =
            Version::parse("4.0.0-alpha.4").expect("Parsing version must work");
        let res = check_contract_ink_compatibility(&ink_version);
        assert!(res.is_ok());
    }
}
