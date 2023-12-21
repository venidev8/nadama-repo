//! Parameters storage

use namada_macros::StorageKeys;

use super::ADDRESS;
use crate::types::address::Address;
use crate::types::storage::{DbKeySeg, Key};

#[derive(StorageKeys)]
struct Keys {
    // ========================================
    // Ethereum bridge parameters
    // ========================================
    /// Sub-key for storing the initial Ethereum block height when
    /// events will first be extracted from.
    eth_start_height: &'static str,
    /// Sub-key for storing the acitve / inactive status of the Ethereum
    /// bridge.
    active_status: &'static str,
    /// Sub-key for storing the minimum confirmations parameter
    min_confirmations: &'static str,
    /// Sub-key for storing the Ethereum address for wNam.
    native_erc20: &'static str,
    /// Sub-lkey for storing the Ethereum address of the bridge contract.
    bridge_contract_address: &'static str,
    // ========================================
    // PoS parameters
    // ========================================
    pos_inflation_amount: &'static str,
    staked_ratio: &'static str,
    // ========================================
    // Core parameters
    // ========================================
    epoch_duration: &'static str,
    epochs_per_year: &'static str,
    implicit_vp: &'static str,
    max_expected_time_per_block: &'static str,
    tx_whitelist: &'static str,
    vp_whitelist: &'static str,
    max_proposal_bytes: &'static str,
    max_tx_bytes: &'static str,
    max_block_gas: &'static str,
    minimum_gas_price: &'static str,
    fee_unshielding_gas_limit: &'static str,
    fee_unshielding_descriptions_limit: &'static str,
    max_signatures_per_transaction: &'static str,
}

/// Returns if the key is a parameter key.
pub fn is_parameter_key(key: &Key) -> bool {
    matches!(&key.segments[0], DbKeySeg::AddressSeg(addr) if addr == &ADDRESS)
}

/// Returns if the key is a protocol parameter key.
pub fn is_protocol_parameter_key(key: &Key) -> bool {
    let segment = match &key.segments[..] {
        [DbKeySeg::AddressSeg(addr), DbKeySeg::StringSeg(segment)]
            if addr == &ADDRESS =>
        {
            segment.as_str()
        }
        _ => return false,
    };
    Keys::ALL.binary_search(&segment).is_ok()
}

/// Returns if the key is an epoch storage key.
pub fn is_epoch_duration_storage_key(key: &Key) -> bool {
    is_epoch_duration_key_at_addr(key, &ADDRESS)
}

/// Returns if the key is the max_expected_time_per_block key.
pub fn is_max_expected_time_per_block_key(key: &Key) -> bool {
    is_max_expected_time_per_block_key_at_addr(key, &ADDRESS)
}

/// Returns if the key is the tx_whitelist key.
pub fn is_tx_whitelist_key(key: &Key) -> bool {
    is_tx_whitelist_key_at_addr(key, &ADDRESS)
}

/// Returns if the key is the vp_whitelist key.
pub fn is_vp_whitelist_key(key: &Key) -> bool {
    is_vp_whitelist_key_at_addr(key, &ADDRESS)
}

/// Returns if the key is the implicit VP key.
pub fn is_implicit_vp_key(key: &Key) -> bool {
    is_implicit_vp_key_at_addr(key, &ADDRESS)
}

/// Returns if the key is the epoch_per_year key.
pub fn is_epochs_per_year_key(key: &Key) -> bool {
    is_epochs_per_year_key_at_addr(key, &ADDRESS)
}

/// Returns if the key is the staked ratio key.
pub fn is_staked_ratio_key(key: &Key) -> bool {
    is_staked_ratio_key_at_addr(key, &ADDRESS)
}

/// Returns if the key is the PoS reward rate key.
pub fn is_pos_inflation_amount_key(key: &Key) -> bool {
    is_pos_inflation_amount_key_at_addr(key, &ADDRESS)
}

/// Returns if the key is the max proposal bytes key.
pub fn is_max_proposal_bytes_key(key: &Key) -> bool {
    is_max_proposal_bytes_key_at_addr(key, &ADDRESS)
}

/// Returns if the key is the max tx bytes key.
pub fn is_max_tx_bytes_key(key: &Key) -> bool {
    is_max_tx_bytes_key_at_addr(key, &ADDRESS)
}

/// Storage key used for epoch parameter.
pub fn get_epoch_duration_storage_key() -> Key {
    get_epoch_duration_key_at_addr(ADDRESS)
}

/// Storage key used for vp whitelist parameter.
pub fn get_vp_whitelist_storage_key() -> Key {
    get_vp_whitelist_key_at_addr(ADDRESS)
}

/// Storage key used for tx whitelist parameter.
pub fn get_tx_whitelist_storage_key() -> Key {
    get_tx_whitelist_key_at_addr(ADDRESS)
}

/// Storage key used for the fee unshielding gas limit
pub fn get_fee_unshielding_gas_limit_key() -> Key {
    get_fee_unshielding_gas_limit_key_at_addr(ADDRESS)
}

/// Storage key used for the fee unshielding descriptions limit
pub fn get_fee_unshielding_descriptions_limit_key() -> Key {
    get_fee_unshielding_descriptions_limit_key_at_addr(ADDRESS)
}

/// Storage key used for max_epected_time_per_block parameter.
pub fn get_max_expected_time_per_block_key() -> Key {
    get_max_expected_time_per_block_key_at_addr(ADDRESS)
}

/// Storage key used for implicit VP parameter.
pub fn get_implicit_vp_key() -> Key {
    get_implicit_vp_key_at_addr(ADDRESS)
}

/// Storage key used for epochs_per_year parameter.
pub fn get_epochs_per_year_key() -> Key {
    get_epochs_per_year_key_at_addr(ADDRESS)
}

/// Storage key used for staked ratio parameter.
pub fn get_staked_ratio_key() -> Key {
    get_staked_ratio_key_at_addr(ADDRESS)
}

/// Storage key used for the inflation amount parameter.
pub fn get_pos_inflation_amount_key() -> Key {
    get_pos_inflation_amount_key_at_addr(ADDRESS)
}

/// Storage key used for the max proposal bytes.
pub fn get_max_proposal_bytes_key() -> Key {
    get_max_proposal_bytes_key_at_addr(ADDRESS)
}

/// Storage key used for the max tx bytes.
pub fn get_max_tx_bytes_key() -> Key {
    get_max_tx_bytes_key_at_addr(ADDRESS)
}

/// Storage key used for the max block gas.
pub fn get_max_block_gas_key() -> Key {
    get_max_block_gas_key_at_addr(ADDRESS)
}

/// Storage key used for the gas cost table
pub fn get_gas_cost_key() -> Key {
    get_minimum_gas_price_key_at_addr(ADDRESS)
}

/// Storage key used for the max signatures per transaction key
pub fn get_max_signatures_per_transaction_key() -> Key {
    get_max_signatures_per_transaction_key_at_addr(ADDRESS)
}
