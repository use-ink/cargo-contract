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

use std::collections::HashMap;

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
struct Compatibility {
    #[serde(rename = "cargo-contract")]
    cargo_contract_compatibility: HashMap<Version, Requirements>,
}

#[derive(Debug, serde::Deserialize)]
struct Requirements {
    #[serde(rename = "ink")]
    ink_requirements: Vec<VersionReq>,
}

/// Checks whether the contract's ink! version is compatible with the cargo-contract
/// binary.
pub fn check_contract_ink_compatibility(ink_version: &Version) -> Result<()> {
    let compatibility_list = include_str!("../compatibility_list.json");
    let compatibility: Compatibility = serde_json::from_str(compatibility_list)?;
    let cargo_contract_version =
        semver::Version::parse(VERSION).expect("Parsing version failed");
    let ink_req = &compatibility
        .cargo_contract_compatibility
        .get(&cargo_contract_version)
        .ok_or(anyhow!(
            "Missing compatibility configuration for cargo-contract: {}",
            cargo_contract_version
        ))?
        .ink_requirements;

    // Ink! requirements can not be empty
    if ink_req.is_empty() {
        bail!(
            "Missing ink! requirements for cargo-contract: {}",
            cargo_contract_version
        );
    }

    // Check if the ink! version matches any of the requirement
    if !ink_req.iter().any(|req| req.matches(ink_version)) {
        // Get required ink! versions
        let ink_required_versions = ink_req
            .iter()
            .map(|req| format!("'{}'", req))
            .collect::<Vec<_>>()
            .join(", ");

        let ink_update_message = format!(
            "update the contract ink! to a version of {}",
            ink_required_versions
        );
        let contract_not_compatible_message = "The cargo-contract is not compatible \
                                                    with the contract's ink! version. Please";

        // Find best cargo-contract version
        let best_cargo_contract_version = compatibility
            .cargo_contract_compatibility
            .iter()
            .filter_map(|(ver, reqs)| {
                if reqs
                    .ink_requirements
                    .iter()
                    .any(|req| req.matches(ink_version))
                {
                    return Some(ver)
                }
                None
            })
            .max_by(|&a, &b| {
                match (!a.pre.is_empty(), !b.pre.is_empty()) {
                    (false, true) => std::cmp::Ordering::Greater,
                    (true, false) => std::cmp::Ordering::Less,
                    (_, _) => a.cmp(b),
                }
            })
            .ok_or(anyhow!(
                "{} {}",
                contract_not_compatible_message,
                ink_update_message
            ))?;

        bail!(
            "{} update the cargo-contract to version \
            '{}' or {}",
            contract_not_compatible_message,
            best_cargo_contract_version,
            ink_update_message
        );
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
            "The cargo-contract is not compatible with the contract's ink! version. \
            Please update the cargo-contract to version '1.5.0' or update \
            the contract ink! to a version of '^4.0.0-alpha.3', '^4.0.0', '^5.0.0-alpha'"
        );

        let ink_version =
            Version::parse("4.0.0-alpha.1").expect("Parsing version must work");
        let res = check_contract_ink_compatibility(&ink_version)
            .expect_err("Ink version check should fail");

        assert_eq!(
                res.to_string(),
                "The cargo-contract is not compatible with the contract's ink! version. \
                Please update the cargo-contract to version '1.5.0' or update \
                the contract ink! to a version of '^4.0.0-alpha.3', '^4.0.0', '^5.0.0-alpha'"
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
