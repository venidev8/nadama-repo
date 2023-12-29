//! Contexts for IBC validity predicate

use std::collections::{BTreeSet, HashMap, HashSet};

use borsh_ext::BorshSerializeExt;
use namada_core::ledger::ibc::{IbcCommonContext, IbcStorageContext};
use namada_core::ledger::masp_utils;

use crate::ledger::ibc::storage::is_ibc_key;
use crate::ledger::native_vp::CtxPreStorageRead;
use crate::ledger::storage::write_log::StorageModification;
use crate::ledger::storage::{self as ledger_storage, StorageHasher};
use crate::ledger::storage_api::{self, StorageRead, StorageWrite};
use crate::types::address::{Address, InternalAddress};
use crate::types::ibc::IbcEvent;
use crate::types::storage::{
    BlockHash, BlockHeight, Epoch, Header, Key, TxIndex,
};
use crate::types::token::{self, Amount, DenominatedAmount};
use crate::vm::WasmCacheAccess;

/// Result of a storage API call.
pub type Result<T> = std::result::Result<T, storage_api::Error>;

/// Pseudo execution environment context for ibc native vp
#[derive(Debug)]
pub struct PseudoExecutionContext<'view, 'a, DB, H, CA>
where
    DB: 'static + ledger_storage::DB + for<'iter> ledger_storage::DBIter<'iter>,
    H: 'static + StorageHasher,
    CA: 'static + WasmCacheAccess,
{
    /// Temporary store for pseudo execution
    store: HashMap<Key, StorageModification>,
    /// Context to read the previous value
    ctx: CtxPreStorageRead<'view, 'a, DB, H, CA>,
    /// IBC event
    pub event: BTreeSet<IbcEvent>,
}

impl<'view, 'a, DB, H, CA> PseudoExecutionContext<'view, 'a, DB, H, CA>
where
    DB: 'static + ledger_storage::DB + for<'iter> ledger_storage::DBIter<'iter>,
    H: 'static + StorageHasher,
    CA: 'static + WasmCacheAccess,
{
    /// Generate new pseudo execution context
    pub fn new(ctx: CtxPreStorageRead<'view, 'a, DB, H, CA>) -> Self {
        Self {
            store: HashMap::new(),
            ctx,
            event: BTreeSet::new(),
        }
    }

    /// Get the set of changed keys
    pub(crate) fn get_changed_keys(&self) -> HashSet<&Key> {
        self.store.keys().filter(|k| is_ibc_key(k)).collect()
    }

    /// Get the changed value
    pub(crate) fn get_changed_value(
        &self,
        key: &Key,
    ) -> Option<&StorageModification> {
        self.store.get(key)
    }
}

impl<'view, 'a, DB, H, CA> StorageRead
    for PseudoExecutionContext<'view, 'a, DB, H, CA>
where
    DB: 'static + ledger_storage::DB + for<'iter> ledger_storage::DBIter<'iter>,
    H: 'static + StorageHasher,
    CA: 'static + WasmCacheAccess,
{
    type PrefixIter<'iter> = ledger_storage::PrefixIter<'iter, DB> where Self: 'iter;

    fn read_bytes(&self, key: &Key) -> Result<Option<Vec<u8>>> {
        match self.store.get(key) {
            Some(StorageModification::Write { ref value }) => {
                Ok(Some(value.clone()))
            }
            Some(StorageModification::Delete) => Ok(None),
            Some(StorageModification::Temp { .. }) => {
                unreachable!("Temp shouldn't be inserted")
            }
            Some(StorageModification::InitAccount { .. }) => {
                unreachable!("InitAccount shouldn't be inserted")
            }
            None => self.ctx.read_bytes(key),
        }
    }

    fn has_key(&self, key: &Key) -> Result<bool> {
        Ok(self.store.contains_key(key) || self.ctx.has_key(key)?)
    }

    fn iter_prefix<'iter>(
        &'iter self,
        prefix: &Key,
    ) -> Result<Self::PrefixIter<'iter>> {
        // NOTE: Read only the previous state since the updated state isn't
        // needed for the caller
        self.ctx.iter_prefix(prefix)
    }

    fn iter_next<'iter>(
        &'iter self,
        iter: &mut Self::PrefixIter<'iter>,
    ) -> Result<Option<(String, Vec<u8>)>> {
        self.ctx.iter_next(iter)
    }

    fn get_chain_id(&self) -> Result<String> {
        self.ctx.get_chain_id()
    }

    fn get_block_height(&self) -> Result<BlockHeight> {
        self.ctx.get_block_height()
    }

    fn get_block_header(&self, height: BlockHeight) -> Result<Option<Header>> {
        self.ctx.get_block_header(height)
    }

    fn get_block_hash(&self) -> Result<BlockHash> {
        self.ctx.get_block_hash()
    }

    fn get_block_epoch(&self) -> Result<Epoch> {
        self.ctx.get_block_epoch()
    }

    fn get_tx_index(&self) -> Result<TxIndex> {
        self.ctx.get_tx_index()
    }

    fn get_native_token(&self) -> Result<Address> {
        self.ctx.get_native_token()
    }
}

impl<'view, 'a, DB, H, CA> StorageWrite
    for PseudoExecutionContext<'view, 'a, DB, H, CA>
where
    DB: 'static + ledger_storage::DB + for<'iter> ledger_storage::DBIter<'iter>,
    H: 'static + StorageHasher,
    CA: 'static + WasmCacheAccess,
{
    fn write_bytes(
        &mut self,
        key: &Key,
        value: impl AsRef<[u8]>,
    ) -> Result<()> {
        self.store.insert(
            key.clone(),
            StorageModification::Write {
                value: value.as_ref().to_vec(),
            },
        );
        Ok(())
    }

    fn delete(&mut self, key: &Key) -> Result<()> {
        self.store.insert(key.clone(), StorageModification::Delete);
        Ok(())
    }
}

impl<'view, 'a, DB, H, CA> IbcStorageContext
    for PseudoExecutionContext<'view, 'a, DB, H, CA>
where
    DB: 'static + ledger_storage::DB + for<'iter> ledger_storage::DBIter<'iter>,
    H: 'static + StorageHasher,
    CA: 'static + WasmCacheAccess,
{
    fn emit_ibc_event(&mut self, event: IbcEvent) -> Result<()> {
        self.event.insert(event);
        Ok(())
    }

    fn get_ibc_events(
        &self,
        event_type: impl AsRef<str>,
    ) -> Result<Vec<IbcEvent>> {
        Ok(self
            .event
            .iter()
            .filter(|event| event.event_type == *event_type.as_ref())
            .cloned()
            .collect())
    }

    fn transfer_token(
        &mut self,
        src: &Address,
        dest: &Address,
        token: &Address,
        amount: DenominatedAmount,
    ) -> Result<()> {
        let src_key = token::balance_key(token, src);
        let dest_key = token::balance_key(token, dest);
        let src_bal: Option<Amount> = self.ctx.read(&src_key)?;
        let mut src_bal = src_bal.expect("The source has no balance");
        src_bal.spend(&amount.amount);
        let mut dest_bal: Amount =
            self.ctx.read(&dest_key)?.unwrap_or_default();
        dest_bal.receive(&amount.amount);

        self.write(&src_key, src_bal.serialize_to_vec())?;
        self.write(&dest_key, dest_bal.serialize_to_vec())
    }

    fn handle_masp_tx(
        &mut self,
        shielded: &masp_primitives::transaction::Transaction,
    ) -> Result<()> {
        masp_utils::handle_masp_tx(self, shielded)?;
        masp_utils::update_note_commitment_tree(self, shielded)
    }

    fn mint_token(
        &mut self,
        target: &Address,
        token: &Address,
        amount: DenominatedAmount,
    ) -> Result<()> {
        let target_key = token::balance_key(token, target);
        let mut target_bal: Amount =
            self.ctx.read(&target_key)?.unwrap_or_default();
        target_bal.receive(&amount.amount);

        let minted_key = token::minted_balance_key(token);
        let mut minted_bal: Amount =
            self.ctx.read(&minted_key)?.unwrap_or_default();
        minted_bal.receive(&amount.amount);

        self.write(&target_key, target_bal.serialize_to_vec())?;
        self.write(&minted_key, minted_bal.serialize_to_vec())?;

        let minter_key = token::minter_key(token);
        self.write(
            &minter_key,
            Address::Internal(InternalAddress::Ibc).serialize_to_vec(),
        )
    }

    fn burn_token(
        &mut self,
        target: &Address,
        token: &Address,
        amount: DenominatedAmount,
    ) -> Result<()> {
        let target_key = token::balance_key(token, target);
        let mut target_bal: Amount =
            self.ctx.read(&target_key)?.unwrap_or_default();
        target_bal.spend(&amount.amount);

        let minted_key = token::minted_balance_key(token);
        let mut minted_bal: Amount =
            self.ctx.read(&minted_key)?.unwrap_or_default();
        minted_bal.spend(&amount.amount);

        self.write(&target_key, target_bal.serialize_to_vec())?;
        self.write(&minted_key, minted_bal.serialize_to_vec())
    }

    fn log_string(&self, message: String) {
        tracing::debug!("{message} in the pseudo execution for IBC VP");
    }
}

impl<'view, 'a, DB, H, CA> IbcCommonContext
    for PseudoExecutionContext<'view, 'a, DB, H, CA>
where
    DB: 'static + ledger_storage::DB + for<'iter> ledger_storage::DBIter<'iter>,
    H: 'static + StorageHasher,
    CA: 'static + WasmCacheAccess,
{
}

/// Ibc native vp validation context
#[derive(Debug)]
pub struct VpValidationContext<'view, 'a, DB, H, CA>
where
    DB: 'static + ledger_storage::DB + for<'iter> ledger_storage::DBIter<'iter>,
    H: 'static + StorageHasher,
    CA: 'static + WasmCacheAccess,
{
    /// Context to read the post value
    ctx: CtxPreStorageRead<'view, 'a, DB, H, CA>,
}

impl<'view, 'a, DB, H, CA> VpValidationContext<'view, 'a, DB, H, CA>
where
    DB: 'static + ledger_storage::DB + for<'iter> ledger_storage::DBIter<'iter>,
    H: 'static + StorageHasher,
    CA: 'static + WasmCacheAccess,
{
    /// Generate a new ibc vp validation context
    pub fn new(ctx: CtxPreStorageRead<'view, 'a, DB, H, CA>) -> Self {
        Self { ctx }
    }
}

impl<'view, 'a, DB, H, CA> StorageRead
    for VpValidationContext<'view, 'a, DB, H, CA>
where
    DB: 'static + ledger_storage::DB + for<'iter> ledger_storage::DBIter<'iter>,
    H: 'static + StorageHasher,
    CA: 'static + WasmCacheAccess,
{
    type PrefixIter<'iter> = ledger_storage::PrefixIter<'iter, DB> where Self: 'iter;

    fn read_bytes(&self, key: &Key) -> Result<Option<Vec<u8>>> {
        self.ctx.read_bytes(key)
    }

    fn has_key(&self, key: &Key) -> Result<bool> {
        self.ctx.has_key(key)
    }

    fn iter_prefix<'iter>(
        &'iter self,
        prefix: &Key,
    ) -> Result<Self::PrefixIter<'iter>> {
        self.ctx.iter_prefix(prefix)
    }

    fn iter_next<'iter>(
        &'iter self,
        iter: &mut Self::PrefixIter<'iter>,
    ) -> Result<Option<(String, Vec<u8>)>> {
        self.ctx.iter_next(iter)
    }

    fn get_chain_id(&self) -> Result<String> {
        self.ctx.get_chain_id()
    }

    fn get_block_height(&self) -> Result<BlockHeight> {
        self.ctx.get_block_height()
    }

    fn get_block_header(&self, height: BlockHeight) -> Result<Option<Header>> {
        self.ctx.get_block_header(height)
    }

    fn get_block_hash(&self) -> Result<BlockHash> {
        self.ctx.get_block_hash()
    }

    fn get_block_epoch(&self) -> Result<Epoch> {
        self.ctx.get_block_epoch()
    }

    fn get_tx_index(&self) -> Result<TxIndex> {
        self.ctx.get_tx_index()
    }

    fn get_native_token(&self) -> Result<Address> {
        self.ctx.get_native_token()
    }
}

impl<'view, 'a, DB, H, CA> StorageWrite
    for VpValidationContext<'view, 'a, DB, H, CA>
where
    DB: 'static + ledger_storage::DB + for<'iter> ledger_storage::DBIter<'iter>,
    H: 'static + StorageHasher,
    CA: 'static + WasmCacheAccess,
{
    fn write_bytes(
        &mut self,
        _key: &Key,
        _val: impl AsRef<[u8]>,
    ) -> Result<()> {
        unimplemented!("Validation doesn't write any data")
    }

    fn delete(&mut self, _key: &Key) -> Result<()> {
        unimplemented!("Validation doesn't delete any data")
    }
}

impl<'view, 'a, DB, H, CA> IbcStorageContext
    for VpValidationContext<'view, 'a, DB, H, CA>
where
    DB: 'static + ledger_storage::DB + for<'iter> ledger_storage::DBIter<'iter>,
    H: 'static + StorageHasher,
    CA: 'static + WasmCacheAccess,
{
    fn emit_ibc_event(&mut self, _event: IbcEvent) -> Result<()> {
        unimplemented!("Validation doesn't emit an event")
    }

    fn get_ibc_events(
        &self,
        _event_type: impl AsRef<str>,
    ) -> Result<Vec<IbcEvent>> {
        unimplemented!("Validation doesn't get an event")
    }

    fn transfer_token(
        &mut self,
        _src: &Address,
        _dest: &Address,
        _token: &Address,
        _amount: DenominatedAmount,
    ) -> Result<()> {
        unimplemented!("Validation doesn't transfer")
    }

    fn handle_masp_tx(
        &mut self,
        _shielded: &masp_primitives::transaction::Transaction,
    ) -> Result<()> {
        unimplemented!("Validation doesn't handle a masp tx")
    }

    fn mint_token(
        &mut self,
        _target: &Address,
        _token: &Address,
        _amount: DenominatedAmount,
    ) -> Result<()> {
        unimplemented!("Validation doesn't mint")
    }

    fn burn_token(
        &mut self,
        _target: &Address,
        _token: &Address,
        _amount: DenominatedAmount,
    ) -> Result<()> {
        unimplemented!("Validation doesn't burn")
    }

    /// Logging
    fn log_string(&self, message: String) {
        tracing::debug!("{message} for validation in IBC VP");
    }
}

impl<'view, 'a, DB, H, CA> IbcCommonContext
    for VpValidationContext<'view, 'a, DB, H, CA>
where
    DB: 'static + ledger_storage::DB + for<'iter> ledger_storage::DBIter<'iter>,
    H: 'static + StorageHasher,
    CA: 'static + WasmCacheAccess,
{
}
