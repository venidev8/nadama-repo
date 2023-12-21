//! Protocol parameters
pub mod storage;

use std::collections::BTreeMap;

use namada_core::borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use namada_core::types::address::{Address, InternalAddress};
use namada_core::types::chain::ProposalBytes;
use namada_core::types::dec::Dec;
use namada_core::types::hash::Hash;
use namada_core::types::time::DurationSecs;
use namada_core::types::token;
use namada_storage::{self, ResultExt, StorageRead, StorageWrite};
use thiserror::Error;

/// The internal address for storage keys representing parameters than
/// can be changed via governance.
pub const ADDRESS: Address = Address::Internal(InternalAddress::Parameters);

/// Protocol parameters
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    BorshSchema,
)]
pub struct Parameters {
    /// Max payload size, in bytes, for a mempool tx.
    pub max_tx_bytes: u32,
    /// Epoch duration (read only)
    pub epoch_duration: EpochDuration,
    /// Maximum expected time per block (read only)
    pub max_expected_time_per_block: DurationSecs,
    /// Max payload size, in bytes, for a tx batch proposal.
    pub max_proposal_bytes: ProposalBytes,
    /// Max gas for block
    pub max_block_gas: u64,
    /// Whitelisted validity predicate hashes (read only)
    pub vp_whitelist: Vec<String>,
    /// Whitelisted tx hashes (read only)
    pub tx_whitelist: Vec<String>,
    /// Implicit accounts validity predicate WASM code hash
    pub implicit_vp_code_hash: Hash,
    /// Expected number of epochs per year (read only)
    pub epochs_per_year: u64,
    /// Maximum number of signature per transaction
    pub max_signatures_per_transaction: u8,
    /// PoS staked ratio (read + write for every epoch)
    pub staked_ratio: Dec,
    /// PoS inflation amount from the last epoch (read + write for every epoch)
    pub pos_inflation_amount: token::Amount,
    /// Fee unshielding gas limit
    pub fee_unshielding_gas_limit: u64,
    /// Fee unshielding descriptions limit
    pub fee_unshielding_descriptions_limit: u64,
    /// Map of the cost per gas unit for every token allowed for fee payment
    pub minimum_gas_price: BTreeMap<Address, token::Amount>,
}

/// Epoch duration. A new epoch begins as soon as both the `min_num_of_blocks`
/// and `min_duration` have passed since the beginning of the current epoch.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    BorshSchema,
)]
pub struct EpochDuration {
    /// Minimum number of blocks in an epoch
    pub min_num_of_blocks: u64,
    /// Minimum duration of an epoch
    pub min_duration: DurationSecs,
}

#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum ReadError {
    #[error("Storage error: {0}")]
    StorageError(namada_storage::Error),
    #[error("Storage type error: {0}")]
    StorageTypeError(namada_core::types::storage::Error),
    #[error("Protocol parameters are missing, they must be always set")]
    ParametersMissing,
}

#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum WriteError {
    #[error("Storage error: {0}")]
    StorageError(namada_storage::Error),
    #[error("Serialize error: {0}")]
    SerializeError(String),
}

impl Parameters {
    /// Initialize parameters in storage in the genesis block.
    pub fn init_storage<S>(&self, storage: &mut S) -> namada_storage::Result<()>
    where
        S: StorageRead + StorageWrite,
    {
        let Self {
            max_tx_bytes,
            epoch_duration,
            max_expected_time_per_block,
            max_proposal_bytes,
            max_block_gas,
            vp_whitelist,
            tx_whitelist,
            implicit_vp_code_hash,
            epochs_per_year,
            max_signatures_per_transaction,
            staked_ratio,
            pos_inflation_amount,
            minimum_gas_price,
            fee_unshielding_gas_limit,
            fee_unshielding_descriptions_limit,
        } = self;

        // write max tx bytes parameter
        let max_tx_bytes_key = storage::get_max_tx_bytes_key();
        storage.write(&max_tx_bytes_key, max_tx_bytes)?;

        // write max proposal bytes parameter
        let max_proposal_bytes_key = storage::get_max_proposal_bytes_key();
        storage.write(&max_proposal_bytes_key, max_proposal_bytes)?;

        // write max block gas parameter
        let max_block_gas_key = storage::get_max_block_gas_key();
        storage.write(&max_block_gas_key, max_block_gas)?;

        // write epoch parameters
        let epoch_key = storage::get_epoch_duration_storage_key();
        storage.write(&epoch_key, epoch_duration)?;

        // write fee unshielding gas limit
        let fee_unshielding_gas_limit_key =
            storage::get_fee_unshielding_gas_limit_key();
        storage
            .write(&fee_unshielding_gas_limit_key, fee_unshielding_gas_limit)?;

        // write fee unshielding descriptions limit
        let fee_unshielding_descriptions_limit_key =
            storage::get_fee_unshielding_descriptions_limit_key();
        storage.write(
            &fee_unshielding_descriptions_limit_key,
            fee_unshielding_descriptions_limit,
        )?;

        // write vp whitelist parameter
        let vp_whitelist_key = storage::get_vp_whitelist_storage_key();
        let vp_whitelist = vp_whitelist
            .iter()
            .map(|id| id.to_lowercase())
            .collect::<Vec<String>>();
        storage.write(&vp_whitelist_key, vp_whitelist)?;

        // write tx whitelist parameter
        let tx_whitelist_key = storage::get_tx_whitelist_storage_key();
        let tx_whitelist = tx_whitelist
            .iter()
            .map(|id| id.to_lowercase())
            .collect::<Vec<String>>();
        storage.write(&tx_whitelist_key, tx_whitelist)?;

        // write max expected time per block
        let max_expected_time_per_block_key =
            storage::get_max_expected_time_per_block_key();
        storage.write(
            &max_expected_time_per_block_key,
            max_expected_time_per_block,
        )?;

        // write implicit vp parameter
        let implicit_vp_key = storage::get_implicit_vp_key();
        // Using `fn write_bytes` here, because implicit_vp code hash doesn't
        // need to be encoded, it's bytes already.
        storage.write_bytes(&implicit_vp_key, implicit_vp_code_hash)?;

        let epochs_per_year_key = storage::get_epochs_per_year_key();
        storage.write(&epochs_per_year_key, epochs_per_year)?;

        let max_signatures_per_transaction_key =
            storage::get_max_signatures_per_transaction_key();
        storage.write(
            &max_signatures_per_transaction_key,
            max_signatures_per_transaction,
        )?;

        let staked_ratio_key = storage::get_staked_ratio_key();
        storage.write(&staked_ratio_key, staked_ratio)?;

        let pos_inflation_key = storage::get_pos_inflation_amount_key();
        storage.write(&pos_inflation_key, pos_inflation_amount)?;

        let gas_cost_key = storage::get_gas_cost_key();
        storage.write(&gas_cost_key, minimum_gas_price)?;

        Ok(())
    }
}

/// Get the max signatures per transactio parameter
pub fn max_signatures_per_transaction<S>(
    storage: &S,
) -> namada_storage::Result<Option<u8>>
where
    S: StorageRead,
{
    let key = storage::get_max_signatures_per_transaction_key();
    storage.read(&key)
}

/// Update the max_expected_time_per_block parameter in storage. Returns the
/// parameters and gas cost.
pub fn update_max_expected_time_per_block_parameter<S>(
    storage: &mut S,
    value: &DurationSecs,
) -> namada_storage::Result<()>
where
    S: StorageRead + StorageWrite,
{
    let key = storage::get_max_expected_time_per_block_key();
    storage.write(&key, value)
}

/// Update the vp whitelist parameter in storage. Returns the parameters and gas
/// cost.
pub fn update_vp_whitelist_parameter<S>(
    storage: &mut S,
    value: Vec<String>,
) -> namada_storage::Result<()>
where
    S: StorageRead + StorageWrite,
{
    let key = storage::get_vp_whitelist_storage_key();
    storage.write(
        &key,
        value
            .iter()
            .map(|id| id.to_lowercase())
            .collect::<Vec<String>>(),
    )
}

/// Update the tx whitelist parameter in storage. Returns the parameters and gas
/// cost.
pub fn update_tx_whitelist_parameter<S>(
    storage: &mut S,
    value: Vec<String>,
) -> namada_storage::Result<()>
where
    S: StorageRead + StorageWrite,
{
    let key = storage::get_tx_whitelist_storage_key();
    storage.write(
        &key,
        value
            .iter()
            .map(|id| id.to_lowercase())
            .collect::<Vec<String>>(),
    )
}

/// Update the epoch parameter in storage. Returns the parameters and gas
/// cost.
pub fn update_epoch_parameter<S>(
    storage: &mut S,
    value: &EpochDuration,
) -> namada_storage::Result<()>
where
    S: StorageRead + StorageWrite,
{
    let key = storage::get_epoch_duration_storage_key();
    storage.write(&key, value)
}

/// Update the epochs_per_year parameter in storage. Returns the parameters and
/// gas cost.
pub fn update_epochs_per_year_parameter<S>(
    storage: &mut S,
    value: &u64,
) -> namada_storage::Result<()>
where
    S: StorageRead + StorageWrite,
{
    let key = storage::get_epochs_per_year_key();
    storage.write(&key, value)
}

/// Update the PoS staked ratio parameter in storage. Returns the parameters and
/// gas cost.
pub fn update_staked_ratio_parameter<S>(
    storage: &mut S,
    value: &Dec,
) -> namada_storage::Result<()>
where
    S: StorageRead + StorageWrite,
{
    let key = storage::get_staked_ratio_key();
    storage.write(&key, value)
}

/// Update the PoS inflation rate parameter in storage. Returns the parameters
/// and gas cost.
pub fn update_pos_inflation_amount_parameter<S>(
    storage: &mut S,
    value: &u64,
) -> namada_storage::Result<()>
where
    S: StorageRead + StorageWrite,
{
    let key = storage::get_pos_inflation_amount_key();
    storage.write(&key, value)
}

/// Update the implicit VP parameter in storage. Return the gas cost.
pub fn update_implicit_vp<S>(
    storage: &mut S,
    implicit_vp: &[u8],
) -> namada_storage::Result<()>
where
    S: StorageRead + StorageWrite,
{
    let key = storage::get_implicit_vp_key();
    // Using `fn write_bytes` here, because implicit_vp doesn't need to be
    // encoded, it's bytes already.
    storage.write_bytes(&key, implicit_vp)
}

/// Update the max signatures per transaction storage parameter
pub fn update_max_signature_per_tx<S>(
    storage: &mut S,
    value: u8,
) -> namada_storage::Result<()>
where
    S: StorageRead + StorageWrite,
{
    let key = storage::get_max_signatures_per_transaction_key();
    // Using `fn write_bytes` here, because implicit_vp doesn't need to be
    // encoded, it's bytes already.
    storage.write(&key, value)
}

/// Read the the epoch duration parameter from store
pub fn read_epoch_duration_parameter<S>(
    storage: &S,
) -> namada_storage::Result<EpochDuration>
where
    S: StorageRead,
{
    // read epoch
    let epoch_key = storage::get_epoch_duration_storage_key();
    let epoch_duration = storage.read(&epoch_key)?;
    epoch_duration
        .ok_or(ReadError::ParametersMissing)
        .into_storage_result()
}

/// Read the cost per unit of gas for the provided token
pub fn read_gas_cost<S>(
    storage: &S,
    token: &Address,
) -> namada_storage::Result<Option<token::Amount>>
where
    S: StorageRead,
{
    let gas_cost_table: BTreeMap<Address, token::Amount> = storage
        .read(&storage::get_gas_cost_key())?
        .ok_or(ReadError::ParametersMissing)
        .into_storage_result()?;
    Ok(gas_cost_table.get(token).map(|amount| amount.to_owned()))
}

/// Read all the parameters from storage. Returns the parameters and gas
/// cost.
pub fn read<S>(storage: &S) -> namada_storage::Result<Parameters>
where
    S: StorageRead,
{
    // read max proposal bytes
    let max_proposal_bytes: ProposalBytes = {
        let key = storage::get_max_proposal_bytes_key();
        let value = storage.read(&key)?;
        value
            .ok_or(ReadError::ParametersMissing)
            .into_storage_result()?
    };

    // read max block gas
    let max_block_gas: u64 = {
        let key = storage::get_max_block_gas_key();
        let value = storage.read(&key)?;
        value
            .ok_or(ReadError::ParametersMissing)
            .into_storage_result()?
    };

    // read epoch duration
    let epoch_duration = read_epoch_duration_parameter(storage)?;

    // read vp whitelist
    let vp_whitelist_key = storage::get_vp_whitelist_storage_key();
    let value = storage.read(&vp_whitelist_key)?;
    let vp_whitelist: Vec<String> = value
        .ok_or(ReadError::ParametersMissing)
        .into_storage_result()?;

    // read tx whitelist
    let tx_whitelist_key = storage::get_tx_whitelist_storage_key();
    let value = storage.read(&tx_whitelist_key)?;
    let tx_whitelist: Vec<String> = value
        .ok_or(ReadError::ParametersMissing)
        .into_storage_result()?;

    // read max expected block time
    let max_expected_time_per_block_key =
        storage::get_max_expected_time_per_block_key();
    let value = storage.read(&max_expected_time_per_block_key)?;
    let max_expected_time_per_block: DurationSecs = value
        .ok_or(ReadError::ParametersMissing)
        .into_storage_result()?;

    let implicit_vp_key = storage::get_implicit_vp_key();
    let value = storage
        .read_bytes(&implicit_vp_key)?
        .ok_or(ReadError::ParametersMissing)
        .into_storage_result()?;
    let implicit_vp_code_hash =
        Hash::try_from(&value[..]).into_storage_result()?;

    // read fee unshielding gas limit
    let fee_unshielding_gas_limit_key =
        storage::get_fee_unshielding_gas_limit_key();
    let value = storage.read(&fee_unshielding_gas_limit_key)?;
    let fee_unshielding_gas_limit: u64 = value
        .ok_or(ReadError::ParametersMissing)
        .into_storage_result()?;

    // read fee unshielding descriptions limit
    let fee_unshielding_descriptions_limit_key =
        storage::get_fee_unshielding_descriptions_limit_key();
    let value = storage.read(&fee_unshielding_descriptions_limit_key)?;
    let fee_unshielding_descriptions_limit: u64 = value
        .ok_or(ReadError::ParametersMissing)
        .into_storage_result()?;

    // read epochs per year
    let epochs_per_year_key = storage::get_epochs_per_year_key();
    let value = storage.read(&epochs_per_year_key)?;
    let epochs_per_year: u64 = value
        .ok_or(ReadError::ParametersMissing)
        .into_storage_result()?;

    // read the maximum signatures per transaction
    let max_signatures_per_transaction_key =
        storage::get_max_signatures_per_transaction_key();
    let value: Option<u8> =
        storage.read(&max_signatures_per_transaction_key)?;
    let max_signatures_per_transaction: u8 = value
        .ok_or(ReadError::ParametersMissing)
        .into_storage_result()?;

    // read staked ratio
    let staked_ratio_key = storage::get_staked_ratio_key();
    let value = storage.read(&staked_ratio_key)?;
    let staked_ratio = value
        .ok_or(ReadError::ParametersMissing)
        .into_storage_result()?;

    // read PoS inflation rate
    let pos_inflation_key = storage::get_pos_inflation_amount_key();
    let value = storage.read(&pos_inflation_key)?;
    let pos_inflation_amount = value
        .ok_or(ReadError::ParametersMissing)
        .into_storage_result()?;

    // read gas cost
    let gas_cost_key = storage::get_gas_cost_key();
    let value = storage.read(&gas_cost_key)?;
    let minimum_gas_price: BTreeMap<Address, token::Amount> = value
        .ok_or(ReadError::ParametersMissing)
        .into_storage_result()?;

    // read max tx bytes
    let max_tx_bytes_key = storage::get_max_tx_bytes_key();
    let value = storage.read(&max_tx_bytes_key)?;
    let max_tx_bytes = value
        .ok_or(ReadError::ParametersMissing)
        .into_storage_result()?;

    Ok(Parameters {
        max_tx_bytes,
        epoch_duration,
        max_expected_time_per_block,
        max_proposal_bytes,
        max_block_gas,
        vp_whitelist,
        tx_whitelist,
        implicit_vp_code_hash,
        epochs_per_year,
        max_signatures_per_transaction,
        staked_ratio,
        pos_inflation_amount,
        minimum_gas_price,
        fee_unshielding_gas_limit,
        fee_unshielding_descriptions_limit,
    })
}

/// Validate the size of a tx.
pub fn validate_tx_bytes<S>(
    storage: &S,
    tx_size: usize,
) -> namada_storage::Result<bool>
where
    S: StorageRead,
{
    let max_tx_bytes: u32 = storage
        .read(&storage::get_max_tx_bytes_key())?
        .expect("The max tx bytes param should be present in storage");
    Ok(tx_size <= max_tx_bytes as usize)
}
