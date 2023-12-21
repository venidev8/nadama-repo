//! Support for signature based authorization of actions on a user account
//! using public key(s) and signature threshold (minimum number of signatures
//! needed to authorize an action) stored on-chain.

mod storage;
mod storage_key;

use std::collections::{BTreeMap, HashMap};

use borsh::{BorshDeserialize, BorshSerialize};
use namada_core::address::Address;
use namada_core::hints;
use namada_core::key::{common, RefTo};
use serde::{Deserialize, Serialize};
pub use storage::*;
pub use storage_key::*;

#[derive(
    Debug, Clone, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
/// Account data
pub struct Account {
    /// The map between indexes and public keys for an account
    pub public_keys_map: AccountPublicKeysMap,
    /// The account signature threshold
    pub threshold: u8,
    /// The address corresponding to the account owner
    pub address: Address,
}

impl Account {
    /// Retrive a public key from the index
    pub fn get_public_key_from_index(
        &self,
        index: u8,
    ) -> Option<common::PublicKey> {
        self.public_keys_map.get_public_key_from_index(index)
    }

    /// Retrive the index of a public key
    pub fn get_index_from_public_key(
        &self,
        public_key: &common::PublicKey,
    ) -> Option<u8> {
        self.public_keys_map.get_index_from_public_key(public_key)
    }
}
