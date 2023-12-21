//! Support for signature based authorization of actions on a user account
//! using public key(s) and signature threshold (minimum number of signatures
//! needed to authorize an action) stored on-chain.

mod storage_key;

use std::collections::{BTreeMap, HashMap};

use borsh::{BorshDeserialize, BorshSerialize};
use namada_core::address::Address;
use namada_core::hints;
use namada_core::key::{common, RefTo};
use serde::{Deserialize, Serialize};
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

#[derive(
    Debug,
    Clone,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    Default,
)]
/// Holds the public key map data as a bimap for efficient quering
pub struct AccountPublicKeysMap {
    /// Hashmap from public key to index
    pub pk_to_idx: HashMap<common::PublicKey, u8>,
    /// Hashmap from index key to public key
    pub idx_to_pk: HashMap<u8, common::PublicKey>,
}

impl FromIterator<common::PublicKey> for AccountPublicKeysMap {
    fn from_iter<T: IntoIterator<Item = common::PublicKey>>(iter: T) -> Self {
        let mut pk_to_idx = HashMap::new();
        let mut idx_to_pk = HashMap::new();

        for (index, public_key) in iter.into_iter().enumerate() {
            if hints::unlikely(index > u8::MAX as usize) {
                panic!(
                    "Only up to 255 signers are allowed in a multisig account"
                );
            }
            pk_to_idx.insert(public_key.to_owned(), index as u8);
            idx_to_pk.insert(index as u8, public_key.to_owned());
        }

        Self {
            pk_to_idx,
            idx_to_pk,
        }
    }
}

impl AccountPublicKeysMap {
    /// Retrive a public key from the index
    pub fn get_public_key_from_index(
        &self,
        index: u8,
    ) -> Option<common::PublicKey> {
        self.idx_to_pk.get(&index).cloned()
    }

    /// Retrive the index of a public key
    pub fn get_index_from_public_key(
        &self,
        public_key: &common::PublicKey,
    ) -> Option<u8> {
        self.pk_to_idx.get(public_key).cloned()
    }

    /// Index the given set of secret keys
    pub fn index_secret_keys(
        &self,
        secret_keys: Vec<common::SecretKey>,
    ) -> BTreeMap<u8, common::SecretKey> {
        secret_keys
            .into_iter()
            .filter_map(|secret_key: common::SecretKey| {
                self.get_index_from_public_key(&secret_key.ref_to())
                    .map(|index| (index, secret_key))
            })
            .collect()
    }
}
