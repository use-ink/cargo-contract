// Copyright 2018-2022 Parity Technologies (UK) Ltd.
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

/// Serializes the given bytes as byte string.
pub fn serialize_as_byte_str<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if bytes.is_empty() {
        // Return empty string without prepended `0x`.
        return serializer.serialize_str("")
    }
    serde_hex::serialize(bytes, serializer)
}

/// Deserializes the given hex string with optional `0x` prefix.
pub fn deserialize_from_byte_str<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Visitor;

    impl<'b> serde::de::Visitor<'b> for Visitor {
        type Value = Vec<u8>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "hex string with optional 0x prefix")
        }

        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
            let result = from_hex(v);
            result.map_err(E::custom)
        }

        fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
            self.visit_str(&v)
        }
    }

    deserializer.deserialize_str(Visitor)
}

/// Deserializes the given hex string with optional `0x` prefix.
pub fn deserialize_from_byte_str_array<'de, D>(
    deserializer: D,
) -> Result<[u8; 32], D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Visitor;

    impl<'b> serde::de::Visitor<'b> for Visitor {
        type Value = [u8; 32];

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "hex string with optional 0x prefix")
        }

        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
            let result = from_hex(v).map_err(E::custom)?;
            if result.len() != 32 {
                Err(E::custom("Expected exactly 32 bytes"))
            } else {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&result[..]);
                Ok(arr)
            }
        }

        fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
            self.visit_str(&v)
        }
    }

    deserializer.deserialize_str(Visitor)
}

fn from_hex(v: &str) -> Result<Vec<u8>, serde_hex::FromHexError> {
    if v.starts_with("0x") {
        serde_hex::from_hex(v)
    } else {
        serde_hex::from_hex(&format!("0x{}", v))
    }
}
