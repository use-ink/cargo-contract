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

use impl_serde::serialize as serde_hex;

/// Marker inserted by the ink! codegen for an error which can't
/// be checked at compile time.
const INK_ENFORCE_ERR: &str = "__ink_enforce_error_";

/// Errors which may occur when forwarding a call is not allowed.
///
/// We insert markers for these errors in the generated contract code.
/// This is necessary since we can't check these errors at compile time
/// of the contract.
/// `cargo-contract` checks the contract code for these error markers
/// when building a contract and fails if it finds markers.
///
/// # Important Note
///
/// This is a copy of the equivalent type in ink!, which currently resides
/// [here](https://github.com/use-ink/ink/blob/master/crates/lang/codegen/src/generator/cross_calling.rs).
/// This type must be compatible with the ink! version in order to decode
/// the error encoded in the marker.
#[derive(scale::Encode, scale::Decode)]
pub enum EnforcedErrors {
    /// The below error represents calling a `&mut self` message in a context that
    /// only allows for `&self` messages. This may happen under certain circumstances
    /// when ink! trait implementations are involved with long-hand calling notation.
    #[codec(index = 1)]
    CannotCallTraitMessage {
        /// The trait that defines the called message.
        trait_ident: String,
        /// The name of the called message.
        message_ident: String,
        /// The selector of the called message.
        message_selector: [u8; 4],
        /// Is `true` if the `self` receiver of the ink! message is `&mut self`.
        message_mut: bool,
    },
    /// The below error represents calling a constructor in a context that does
    /// not allow calling it. This may happen when the constructor defined in a
    /// trait is cross-called in another contract.
    /// This is not allowed since the contract to which a call is forwarded must
    /// already exist at the point when the call to it is made.
    #[codec(index = 2)]
    CannotCallTraitConstructor {
        /// The trait that defines the called constructor.
        trait_ident: String,
        /// The name of the called constructor.
        constructor_ident: String,
        /// The selector of the called constructor.
        constructor_selector: [u8; 4],
    },
}

/// Extracts the ink! linker error marker from the `field`, parses it, and
/// returns a human-readable error message for it.
pub fn parse_linker_error(field: &str) -> String {
    let encoded = field
        .strip_prefix(INK_ENFORCE_ERR)
        .expect("error marker must exist as prefix");
    let hex = serde_hex::from_hex(encoded).expect("decoding hex failed");
    let decoded = <EnforcedErrors as scale::Decode>::decode(&mut &hex[..]).expect(
        "The `EnforcedError` object could not be decoded. The probable\
        cause is a mismatch between the ink! definition of the type and the\
        local `cargo-contract` definition.",
    );

    match decoded {
        EnforcedErrors::CannotCallTraitMessage {
            trait_ident,
            message_ident,
            message_selector,
            message_mut,
        } => {
            let receiver = match message_mut {
                true => "&mut self",
                false => "&self",
            };
            format!(
                "An error was found while compiling the contract:\n\
                The ink! message `{}::{}` with the selector `{}` contains an invalid trait call.\n\n\
                Please check if the receiver of the function to call is consistent \
                with the scope in which it is called. The receiver is `{}`.",
                trait_ident,
                message_ident,
                serde_hex::to_hex(&scale::Encode::encode(&message_selector), false),
                receiver
            )
        }
        EnforcedErrors::CannotCallTraitConstructor {
            trait_ident,
            constructor_ident,
            constructor_selector,
        } => {
            format!(
                "An error was found while compiling the contract:\n\
                The ink! constructor `{}::{}` with the selector `{}` contains an invalid trait call.\n\
                Constructor never need to be forwarded, please check if this is the case.",
                trait_ident,
                constructor_ident,
                serde_hex::to_hex(&scale::Encode::encode(&constructor_selector), false)
            )
        }
    }
}
