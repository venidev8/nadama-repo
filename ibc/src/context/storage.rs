//! IBC storage context

pub use ics23::ProofSpec;

use crate::ledger::storage_api::{Error, StorageRead, StorageWrite};
use crate::types::address::Address;
use crate::types::ibc::{IbcEvent, IbcShieldedTransfer};
use crate::types::token::DenominatedAmount;

/// IBC context trait to be implemented in integration that can read and write
pub trait IbcStorageContext: StorageRead + StorageWrite {
    /// Emit an IBC event
    fn emit_ibc_event(&mut self, event: IbcEvent) -> Result<(), Error>;

    /// Get IBC events
    fn get_ibc_events(
        &self,
        event_type: impl AsRef<str>,
    ) -> Result<Vec<IbcEvent>, Error>;

    /// Transfer token
    fn transfer_token(
        &mut self,
        src: &Address,
        dest: &Address,
        token: &Address,
        amount: DenominatedAmount,
    ) -> Result<(), Error>;

    /// Handle masp tx
    fn handle_masp_tx(
        &mut self,
        shielded: &IbcShieldedTransfer,
    ) -> Result<(), Error>;

    /// Mint token
    fn mint_token(
        &mut self,
        target: &Address,
        token: &Address,
        amount: DenominatedAmount,
    ) -> Result<(), Error>;

    /// Burn token
    fn burn_token(
        &mut self,
        target: &Address,
        token: &Address,
        amount: DenominatedAmount,
    ) -> Result<(), Error>;

    /// Logging
    fn log_string(&self, message: String);
}
