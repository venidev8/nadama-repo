//! Cryptographic signature keys storage API

use super::*;
use crate::types::account::AccountPublicKeysMap;
use crate::types::address::Address;
use crate::types::key::*;
use crate::types::storage::Key;

/// Init the subspace of a new account
pub fn init_account_storage<S>(
    storage: &mut S,
    owner: &Address,
    public_keys: &[common::PublicKey],
    threshold: u8,
) -> Result<()>
where
    S: StorageWrite + StorageRead,
{
    for (index, public_key) in public_keys.iter().enumerate() {
        let index = index as u8;
        pks_handle(owner).insert(storage, index, public_key.clone())?;
    }
    let threshold_key = threshold_key(owner);
    storage.write(&threshold_key, threshold)
}

/// Get the threshold associated with an account
pub fn threshold<S>(storage: &S, owner: &Address) -> Result<Option<u8>>
where
    S: StorageRead,
{
    let threshold_key = threshold_key(owner);
    storage.read(&threshold_key)
}

/// Get the public keys index map associated with an account
pub fn public_keys<S>(
    storage: &S,
    owner: &Address,
) -> Result<Vec<common::PublicKey>>
where
    S: StorageRead,
{
    let public_keys = pks_handle(owner)
        .iter(storage)?
        .filter_map(|data| match data {
            Ok((_index, public_key)) => Some(public_key),
            Err(_) => None,
        })
        .collect::<Vec<common::PublicKey>>();

    Ok(public_keys)
}

/// Get the public key index map associated with an account
pub fn public_keys_index_map<S>(
    storage: &S,
    owner: &Address,
) -> Result<AccountPublicKeysMap>
where
    S: StorageRead,
{
    let public_keys = public_keys(storage, owner)?;

    Ok(AccountPublicKeysMap::from_iter(public_keys))
}

/// Check if an account exists in storage
pub fn exists<S>(storage: &S, owner: &Address) -> Result<bool>
where
    S: StorageRead,
{
    let vp_key = Key::validity_predicate(owner);
    storage.has_key(&vp_key)
}

/// Set public key at specific index
pub fn set_public_key_at<S>(
    storage: &mut S,
    owner: &Address,
    public_key: &common::PublicKey,
    index: u8,
) -> Result<()>
where
    S: StorageWrite + StorageRead,
{
    pks_handle(owner).insert(storage, index, public_key.clone())?;
    Ok(())
}

/// Clear the public keys account subtorage space
pub fn clear_public_keys<S>(storage: &mut S, owner: &Address) -> Result<()>
where
    S: StorageWrite + StorageRead,
{
    let total_pks = pks_handle(owner).len(storage)?;
    for index in 0..total_pks as u8 {
        pks_handle(owner).remove(storage, &index)?;
    }
    Ok(())
}