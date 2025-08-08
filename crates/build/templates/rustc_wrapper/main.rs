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

use std::env;
use std::process::{Command, exit};

fn main() {
    // First arg is this executable so we skip it.
    let mut args = env::args().skip(1);

    // Setting `RUSTC_WRAPPER` causes `cargo` to pass 'rustc' executable as the next argument.
    let rustc = args.next().unwrap();

    // Composes `rustc` command.
    let mut cmd = Command::new(rustc);
    cmd.envs(env::vars());
    cmd.args(args);

    // Passes extra flags to `rustc`.
    // This is useful in cases where `cargo` won't pass compiler flags to `rustc`
    // for some compiler invocations
    // (e.g. `cargo` doesn't pass `rustc` flags to proc macros and build scripts
    // when the `--target` flag is set).
    // Ref: <https://doc.rust-lang.org/cargo/reference/config.html#buildrustflags>
    if let Ok(rustflags) = env::var("RUSTC_WRAPPER_ENCODED_FLAGS") {
        cmd.args(rustflags.split('\x1f'));
    }

    // Runs `rustc`.
    let exit_status = cmd.status().expect("Failed to spawn `rustc` process");
    if !exit_status.success() {
        // Exits with appropriate exit code on failure.
        exit(exit_status.code().unwrap_or(-1));
    }
}
