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

use crate::contract_storage::{
    ContractStorageData,
    ContractStorageLayout,
};
use contract_transcode::ContractMessageTranscoder;

use ink::{
    metadata::{
        layout::{
            Layout::{
                self,
                Struct,
            },
            LayoutKey,
            RootLayout,
        },
        ConstructorSpec,
        ContractSpec,
        InkProject,
        LangError,
        MessageSpec,
        ReturnTypeSpec,
        TypeSpec,
    },
    storage::{
        traits::{
            ManualKey,
            Storable,
            StorageLayout,
        },
        Lazy,
        Mapping,
    },
    ConstructorResult,
    MessageResult,
};

use scale::Encode;
use std::collections::BTreeMap;
use subxt::backend::legacy::rpc_methods::Bytes;

const BASE_KEY_RAW: [u8; 16] = [0u8; 16];
const ROOT_KEY: u32 = 0;
const LAZY_TYPE_ROOT_KEY: u32 = 1;

fn contract_default_spec() -> ContractSpec {
    ContractSpec::new()
        .constructors(vec![ConstructorSpec::from_label("new")
            .selector([94u8, 189u8, 136u8, 214u8])
            .payable(true)
            .args(Vec::new())
            .returns(ReturnTypeSpec::new(TypeSpec::with_name_str::<
                ConstructorResult<()>,
            >(
                "ink_primitives::ConstructorResult"
            )))
            .docs(Vec::new())
            .done()])
        .messages(vec![MessageSpec::from_label("inc")
            .selector([231u8, 208u8, 89u8, 15u8])
            .mutates(true)
            .payable(true)
            .args(Vec::new())
            .returns(ReturnTypeSpec::new(TypeSpec::with_name_str::<
                MessageResult<()>,
            >(
                "ink_primitives::MessageResult"
            )))
            .default(true)
            .done()])
        .events(Vec::new())
        .lang_error(TypeSpec::with_name_segs::<LangError, _>(
            ::core::iter::Iterator::map(
                ::core::iter::IntoIterator::into_iter(["ink", "LangError"]),
                ::core::convert::AsRef::as_ref,
            ),
        ))
        .done()
}

fn encode_storage_value<T: Storable>(value: &T) -> Bytes {
    let mut value_encoded = Vec::new();
    Storable::encode(value, &mut value_encoded);
    Bytes::from(value_encoded)
}

#[test]
fn storage_decode_simple_type_works() {
    let root_key_encoded = Encode::encode(&ROOT_KEY);
    #[derive(scale_info::TypeInfo, StorageLayout, Storable)]
    struct Data {
        a: i32,
    }

    let Struct(data_layout) = <Data as StorageLayout>::layout(&ROOT_KEY) else {
        panic!("Layout shall be created");
    };
    let storage_layout: Layout = RootLayout::new(
        LayoutKey::from(ROOT_KEY),
        data_layout,
        scale_info::meta_type::<Data>(),
    )
    .into();

    let metadata = InkProject::new(storage_layout, contract_default_spec());
    let decoder = ContractMessageTranscoder::new(metadata);

    let key = [BASE_KEY_RAW.to_vec(), root_key_encoded].concat();
    let value = Data { a: 16 };

    let mut map = BTreeMap::new();
    map.insert(Bytes::from(key), encode_storage_value(&value));
    let data = ContractStorageData::new(map);
    let layout = ContractStorageLayout::new(data, &decoder)
        .expect("Contract storage layout shall be created");

    let cell = layout.iter().next().expect("Root cell shall be in layout");
    assert_eq!(cell.to_string(), format!("Data {{ a: {} }}", value.a));
}

#[test]
fn storage_decode_lazy_type_works() {
    let root_key_encoded = Encode::encode(&ROOT_KEY);
    let lazy_type_root_encoded = Encode::encode(&LAZY_TYPE_ROOT_KEY);
    #[derive(scale_info::TypeInfo, StorageLayout, Storable)]
    struct Data {
        a: Lazy<i32, ManualKey<LAZY_TYPE_ROOT_KEY>>,
    }

    let Struct(data_layout) = <Data as StorageLayout>::layout(&ROOT_KEY) else {
        panic!("Layout shall be created");
    };
    let storage_layout: Layout = RootLayout::new(
        LayoutKey::from(ROOT_KEY),
        data_layout,
        scale_info::meta_type::<Data>(),
    )
    .into();

    let metadata = InkProject::new(storage_layout, contract_default_spec());
    let decoder = ContractMessageTranscoder::new(metadata);

    let key = [BASE_KEY_RAW.to_vec(), root_key_encoded.clone()].concat();
    let lazy_type_key = [BASE_KEY_RAW.to_vec(), lazy_type_root_encoded.clone()].concat();

    let value = Data { a: Lazy::new() };
    // Cannot be set on struct directly because it issues storage calls
    let a = 8i32;

    let mut map = BTreeMap::new();
    map.insert(Bytes::from(key), encode_storage_value(&value));
    map.insert(Bytes::from(lazy_type_key), encode_storage_value(&a));

    let data = ContractStorageData::new(map);
    let layout = ContractStorageLayout::new(data, &decoder)
        .expect("Contract storage layout shall be created");
    let mut iter = layout.iter();
    let cell = iter.next().expect("Root cell shall be in layout");
    assert_eq!(cell.to_string(), "Data { a: Lazy }".to_string());
    assert_eq!(cell.root_key(), hex::encode(root_key_encoded));

    let cell = iter.next().expect("Lazy type cell shall be in layout");
    assert_eq!(cell.to_string(), format!("Lazy {{ {a} }}"));
    assert_eq!(cell.root_key(), hex::encode(lazy_type_root_encoded));
}

#[test]
fn storage_decode_mapping_type_works() {
    let root_key_encoded = Encode::encode(&ROOT_KEY);
    let lazy_type_root_encoded = Encode::encode(&LAZY_TYPE_ROOT_KEY);
    #[derive(scale_info::TypeInfo, StorageLayout, Storable)]
    struct Data {
        a: Mapping<u8, u8, ManualKey<LAZY_TYPE_ROOT_KEY>>,
    }

    let Struct(data_layout) = <Data as StorageLayout>::layout(&ROOT_KEY) else {
        panic!("Layout shall be created");
    };
    let storage_layout: Layout = RootLayout::new(
        LayoutKey::from(ROOT_KEY),
        data_layout,
        scale_info::meta_type::<Data>(),
    )
    .into();

    let metadata = InkProject::new(storage_layout, contract_default_spec());
    let decoder = ContractMessageTranscoder::new(metadata);

    let value = Data { a: Mapping::new() };
    // Cannot be set on struct directly because it issues storage calls
    let mapping_item = (4u8, 8u8);

    let key = [BASE_KEY_RAW.to_vec(), root_key_encoded.clone()].concat();
    let lazy_type_key = [
        BASE_KEY_RAW.to_vec(),
        lazy_type_root_encoded.clone(),
        Encode::encode(&mapping_item.0),
    ]
    .concat();

    let mut map = BTreeMap::new();
    map.insert(Bytes::from(key), encode_storage_value(&value));
    map.insert(
        Bytes::from(lazy_type_key),
        encode_storage_value(&mapping_item.1),
    );

    let data = ContractStorageData::new(map);
    let layout = ContractStorageLayout::new(data, &decoder)
        .expect("Contract storage layout shall be created");
    let mut iter = layout.iter();
    let cell = iter.next().expect("Root cell shall be in layout");
    assert_eq!(cell.to_string(), "Data { a: Mapping }".to_string());
    assert_eq!(cell.root_key(), hex::encode(root_key_encoded));

    let cell = iter.next().expect("Mapping type cell shall be in layout");
    assert_eq!(
        cell.to_string(),
        format!("Mapping {{ {} => {} }}", mapping_item.0, mapping_item.1)
    );
    assert_eq!(cell.root_key(), hex::encode(lazy_type_root_encoded));
}
