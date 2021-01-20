// Copyright 2018-2021 Parity Technologies (UK) Ltd.
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

use toml::value;

/// Subset of cargo profile settings to configure defaults for building contracts
pub struct Profile {
    opt_level: OptLevel,
    lto: Lto,
    // `None` means use rustc default.
    codegen_units: Option<u32>,
    overflow_checks: bool,
    panic: PanicStrategy,
}

impl Profile {
    /// The preferred set of defaults for compiling a release build of a contract
    pub fn default_contract_release() -> Profile {
        Profile {
            opt_level: OptLevel::Z,
            lto: Lto::Fat,
            codegen_units: Some(1),
            overflow_checks: true,
            panic: PanicStrategy::Abort,
        }
    }

    /// Set any unset profile settings from the config.
    ///
    /// Therefore:
    ///   - If the user has explicitly defined a profile setting, it will not be overwritten.
    ///   - If a profile setting is not defined, the value from this profile instance will be added
    pub(super) fn merge(&self, profile: &mut value::Table) {
        let mut set_value_if_vacant = |key: &'static str, value: value::Value| {
            if !profile.contains_key(key) {
                profile.insert(key.into(), value);
            }
        };
        set_value_if_vacant("opt-level", self.opt_level.to_toml_value());
        set_value_if_vacant("lto", self.lto.to_toml_value());
        if let Some(codegen_units) = self.codegen_units {
            set_value_if_vacant("codegen-units", codegen_units.into());
        }
        set_value_if_vacant("overflow-checks", self.overflow_checks.into());
        set_value_if_vacant("panic", self.panic.to_toml_value());
    }
}

/// The [`opt-level`](https://doc.rust-lang.org/cargo/reference/profiles.html#opt-level) setting
#[allow(unused)]
#[derive(Clone, Copy)]
pub enum OptLevel {
    NoOptimizations,
    O1,
    O2,
    O3,
    S,
    Z,
}

impl OptLevel {
    fn to_toml_value(&self) -> value::Value {
        match self {
            OptLevel::NoOptimizations => 0.into(),
            OptLevel::O1 => 1.into(),
            OptLevel::O2 => 2.into(),
            OptLevel::O3 => 3.into(),
            OptLevel::S => "s".into(),
            OptLevel::Z => "z".into(),
        }
    }
}

/// The [`link-time-optimization`](https://doc.rust-lang.org/cargo/reference/profiles.html#lto) setting.
#[derive(Clone, Copy)]
#[allow(unused)]
pub enum Lto {
    /// Sets `lto = false`
    ThinLocal,
    /// Sets `lto = "fat"`, the equivalent of `lto = true`
    Fat,
    /// Sets `lto = "thin"`
    Thin,
    /// Sets `lto = "off"`
    Off,
}

impl Lto {
    fn to_toml_value(&self) -> value::Value {
        match self {
            Lto::ThinLocal => false.into(),
            Lto::Fat => "fat".into(),
            Lto::Thin => "thin".into(),
            Lto::Off => "off".into(),
        }
    }
}

/// The `panic` setting.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
#[allow(unused)]
pub enum PanicStrategy {
    Unwind,
    Abort,
}

impl PanicStrategy {
    fn to_toml_value(&self) -> value::Value {
        match self {
            PanicStrategy::Unwind => "unwind".into(),
            PanicStrategy::Abort => "abort".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn merge_profile_inserts_preferred_defaults() {
        let profile = Profile::default_contract_release();

        // no `[profile.release]` section specified
        let manifest_toml = "";
        let mut expected = toml::value::Table::new();
        expected.insert("opt-level".into(), value::Value::String("z".into()));
        expected.insert("lto".into(), value::Value::String("fat".into()));
        expected.insert("codegen-units".into(), value::Value::Integer(1));
        expected.insert("overflow-checks".into(), value::Value::Boolean(true));
        expected.insert("panic".into(), value::Value::String("abort".into()));

        let mut manifest_profile = toml::from_str(manifest_toml).unwrap();

        profile.merge(&mut manifest_profile);

        assert_eq!(expected, manifest_profile)
    }

    #[test]
    fn merge_profile_preserves_user_defined_settings() {
        let profile = Profile::default_contract_release();

        let manifest_toml = r#"
            panic = "unwind"
            lto = false
            opt-level = 3
            overflow-checks = false
            codegen-units = 256
        "#;
        let mut expected = toml::value::Table::new();
        expected.insert("opt-level".into(), value::Value::Integer(3));
        expected.insert("lto".into(), value::Value::Boolean(false));
        expected.insert("codegen-units".into(), value::Value::Integer(256));
        expected.insert("overflow-checks".into(), value::Value::Boolean(false));
        expected.insert("panic".into(), value::Value::String("unwind".into()));

        let mut manifest_profile = toml::from_str(manifest_toml).unwrap();

        profile.merge(&mut manifest_profile);

        assert_eq!(expected, manifest_profile)
    }
}
