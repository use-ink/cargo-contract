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
struct CompatibilityMap {
    #[serde(rename = "cargo-contract")]
    cargo_contract: HashMap<Version, Compatibility>,
}

#[derive(Debug, serde::Deserialize)]
struct Compatibility {
    ink: Vec<VersionReq>,
}

/// Checks whether the contract's ink! version is compatible with the cargo-contract
/// binary.
pub fn check_contract_ink_compatibility(ink_version: &Version) -> Result<()> {
    let compatibility_list = include_str!("../compatibility_list.json");
    let compatibility_map: CompatibilityMap = serde_json::from_str(compatibility_list)?;
    let cargo_contract_version =
        semver::Version::parse(VERSION).expect("Parsing version failed");
    let ink_req = &compatibility_map
        .cargo_contract
        .get(&cargo_contract_version)
        .ok_or(anyhow!(
            "Missing compatibility configuration for cargo-contract: {}",
            cargo_contract_version
        ))?
        .ink;

    // Check if the ink! version matches any of the requirement
    if !ink_req.iter().any(|req| req.matches(ink_version)) {
        // Get matching ink! versions
        let ink_matches = ink_req
            .iter()
            .map(|req| format!("'{}'", req))
            .collect::<Vec<_>>()
            .join(", ");

        let ink_matches_message = if !ink_matches.is_empty() {
            format!("update the contract ink! to a version of {}", ink_matches)
        } else {
            String::default()
        };

        // Find best cargo-contract version
        let cargo_contract_match_message = compatibility_map
            .cargo_contract
            .iter()
            .filter_map(|(ver, comp)| {
                if comp.ink.iter().any(|req| req.matches(ink_version)) {
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
            .map(|ver| format!("update the cargo-contract to version '{}'", ver))
            .unwrap_or_default();

        let matches_message = [cargo_contract_match_message, ink_matches_message]
            .into_iter()
            .filter(|m| !m.is_empty())
            .collect::<Vec<_>>()
            .join(" or ");
        if !matches_message.is_empty() {
            return Err(anyhow!(
                "The cargo-contract is not compatible with the contract's ink! version. Please {}",
                 matches_message
            ))
        } else {
            bail!(
                "The cargo-contract is not compatible with the contract's ink! version, \
                but a matching version could not be found"
            )
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
            "The cargo-contract is not compatible with the contract's ink! version. \
            Please update the cargo-contract to version '1.5.0' or \
            update the contract ink! to a version of '^4.0.0-alpha.3', '^4.0.0', '^5.0.0-alpha'"
        );

        let ink_version =
            Version::parse("4.0.0-alpha.1").expect("Parsing version must work");
        let res = check_contract_ink_compatibility(&ink_version)
            .expect_err("Ink version check should fail");

        assert_eq!(
                res.to_string(),
                "The cargo-contract is not compatible with the contract's ink! version. \
                Please update the cargo-contract to version '1.5.0' or \
                update the contract ink! to a version of '^4.0.0-alpha.3', '^4.0.0', '^5.0.0-alpha'"
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
