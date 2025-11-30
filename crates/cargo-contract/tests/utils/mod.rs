// Copyright (C) ink! contributors.
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

use std::path::Path;

pub mod node_proc;

/// Create a `cargo contract` command
pub fn cargo_contract<P: AsRef<Path>>(path: P) -> assert_cmd::Command {
    let mut cmd = if let Ok(nextest_bin) = std::env::var("NEXTEST_BIN_EXE_cargo-contract") {
        // When running with nextest archive, use NEXTEST_BIN_EXE_* which has the
        // correct remapped path
        assert_cmd::Command::new(nextest_bin)
    } else {
        // Fall back to cargo_bin_cmd! for normal cargo test
        assert_cmd::cargo::cargo_bin_cmd!(env!("CARGO_PKG_NAME"))
    };
    cmd.current_dir(path).arg("contract");
    cmd
}
