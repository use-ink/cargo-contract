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
    BuildMetadata,
    Comparator,
    Op,
    Version,
    VersionReq,
};

/// Version of the currently executing `cargo-contract` binary.
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, serde::Deserialize)]
struct CompatibilityList {
    versions: Vec<CompatibilityInfo>,
}

#[derive(Debug, serde::Deserialize)]
struct CompatibilityInfo {
    #[serde(rename = "cargo-contract")]
    cargo_contract_req: VersionReq,
    #[serde(rename = "ink")]
    ink_req: VersionReq,
}

trait CustomMatch {
    fn matches_including_pre_releases(&self, version: &Version) -> bool;
}

// The VersionReq matching mechanism behaves in such a way that 4.0.0-alpha.1 does not
// satisfy the requirement ">2.0.0,<5.0.0". To modify this behavior, a custom matching
// implementation has been provided.
impl CustomMatch for VersionReq {
    fn matches_including_pre_releases(&self, version: &Version) -> bool {
        for cmp in &self.comparators {
            let cmp_ver = &comparator_into_version(cmp);
            let res = match cmp.op {
                Op::Exact => cmp_ver == version,
                Op::Greater => cmp_ver < version,
                Op::GreaterEq => cmp_ver <= version,
                Op::Less => cmp_ver > version,
                Op::LessEq => cmp_ver >= version,
                _ => {
                    panic!(
                    "comparison operator in the version requirements is not supported"
                )
                }
            };
            if !res {
                return false
            }
        }
        true
    }
}

fn comparator_into_version(cmp: &Comparator) -> Version {
    Version {
        major: cmp.major,
        minor: cmp
            .minor
            .expect("minor version number needs to be provided in version requirements"),
        patch: cmp
            .patch
            .expect("patch version number needs to be provided in version requirements"),
        pre: cmp.pre.clone(),
        build: BuildMetadata::EMPTY,
    }
}

/// Checks whether the contract's ink! version is compatible with the cargo-contract
/// binary.
pub fn check_contract_ink_compatibility(ink_version: &Version) -> Result<()> {
    let compatibility = include_str!("../compatibility_list.json");
    let compatibility_list: CompatibilityList = serde_json::from_str(compatibility)?;
    let cargo_contract_version =
        semver::Version::parse(VERSION).expect("Parsing version failed");

    let compatible = compatibility_list.versions.iter().find(|&e| {
        matches!(
            (
                e.cargo_contract_req
                    .matches_including_pre_releases(&cargo_contract_version),
                e.ink_req.matches_including_pre_releases(ink_version)
            ),
            (true, true)
        )
    });

    // Compatible versions has not been found
    if compatible.is_none() {
        let matching_req = compatibility_list
            .versions
            .iter()
            .filter_map(|e| {
                match (
                    e.cargo_contract_req
                        .matches_including_pre_releases(&cargo_contract_version),
                    e.ink_req.matches_including_pre_releases(ink_version),
                ) {
                    (true, false) => {
                        Some(format!(
                            "update the contract ink! to version '{}'",
                            e.ink_req
                        ))
                    }
                    (false, true) => {
                        Some(format!(
                            "update the cargo-contract to version '{}'",
                            e.cargo_contract_req
                        ))
                    }
                    (_, _) => None,
                }
            })
            .collect::<Vec<_>>();
        if !matching_req.is_empty() {
            return Err(anyhow!(
                "The cargo-contract is not compatible with the contract's ink! version. Please {}",
                matching_req.join(" or ")
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
            "The cargo-contract is not compatible with the contract's ink! version. Please update the cargo-contract to version '>=1.0.0, <2.0.0-alpha.3' or update the contract ink! to version '>=4.0.0-alpha.3'"
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
