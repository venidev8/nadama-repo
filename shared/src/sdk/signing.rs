//! Functions to sign transactions
use std::collections::{BTreeMap, HashMap};
use std::fmt::Display;
use std::path::PathBuf;

use borsh::{BorshDeserialize, BorshSerialize};
use data_encoding::HEXLOWER;
use itertools::Itertools;
use masp_primitives::asset_type::AssetType;
use masp_primitives::transaction::components::sapling::fees::{
    InputView, OutputView,
};
use namada_core::proto::SignatureIndex;
use namada_core::types::account::AccountPublicKeysMap;
use namada_core::types::address::{
    masp, masp_tx_key, Address, ImplicitAddress,
};
use namada_core::types::token;
// use namada_core::types::storage::Key;
use namada_core::types::token::{Amount, DenominatedAmount, MaspDenom};
use namada_core::types::transaction::pos;
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use zeroize::Zeroizing;

use super::masp::{ShieldedContext, ShieldedTransfer, ShieldedUtils};
use super::rpc::validate_amount;
use crate::core::ledger::governance::storage::proposal::ProposalType;
use crate::core::ledger::governance::storage::vote::{
    StorageProposalVote, VoteType,
};
use crate::display_line;
use crate::ibc::applications::transfer::msgs::transfer::MsgTransfer;
use crate::ibc_proto::google::protobuf::Any;
use crate::ledger::parameters::storage as parameter_storage;
use crate::proto::{MaspBuilder, Section, Tx};
use crate::sdk::error::{EncodingError, Error, TxError};
use crate::sdk::masp::make_asset_type;
use crate::sdk::rpc::{format_denominated_amount, query_wasm_code_hash};
use crate::sdk::tx::{
    TX_BOND_WASM, TX_CHANGE_COMMISSION_WASM, TX_IBC_WASM, TX_INIT_ACCOUNT_WASM,
    TX_INIT_PROPOSAL, TX_INIT_VALIDATOR_WASM, TX_REVEAL_PK, TX_TRANSFER_WASM,
    TX_UNBOND_WASM, TX_UNJAIL_VALIDATOR_WASM, TX_UPDATE_ACCOUNT_WASM,
    TX_VOTE_PROPOSAL, TX_WITHDRAW_WASM, VP_USER_WASM,
};
pub use crate::sdk::wallet::store::AddressVpType;
use crate::sdk::wallet::{Wallet, WalletUtils};
use crate::sdk::{args, rpc};
use crate::types::io::*;
use crate::types::key::*;
use crate::types::masp::{ExtendedViewingKey, PaymentAddress};
use crate::types::storage::Epoch;
use crate::types::token::Transfer;
use crate::types::transaction::account::{InitAccount, UpdateAccount};
use crate::types::transaction::governance::{
    InitProposalData, VoteProposalData,
};
use crate::types::transaction::pos::InitValidator;
use crate::types::transaction::Fee;

#[cfg(feature = "std")]
/// Env. var specifying where to store signing test vectors
const ENV_VAR_LEDGER_LOG_PATH: &str = "NAMADA_LEDGER_LOG_PATH";
#[cfg(feature = "std")]
/// Env. var specifying where to store transaction debug outputs
const ENV_VAR_TX_LOG_PATH: &str = "NAMADA_TX_LOG_PATH";

/// A struture holding the signing data to craft a transaction
#[derive(Clone)]
pub struct SigningTxData {
    /// The address owning the transaction
    pub owner: Option<Address>,
    /// The public keys associated to an account
    pub public_keys: Vec<common::PublicKey>,
    /// The threshold associated to an account
    pub threshold: u8,
    /// The public keys to index map associated to an account
    pub account_public_keys_map: Option<AccountPublicKeysMap>,
    /// The public keys of the fee payer
    pub fee_payer: common::PublicKey,
}

/// Find the public key for the given address and try to load the keypair
/// for it from the wallet. If the keypair is encrypted but a password is not
/// supplied, then it is interactively prompted. Errors if the key cannot be
/// found or loaded.
pub async fn find_pk<
    C: crate::ledger::queries::Client + Sync,
    U: WalletUtils,
    IO: Io,
>(
    client: &C,
    wallet: &mut Wallet<U>,
    addr: &Address,
    password: Option<Zeroizing<String>>,
) -> Result<common::PublicKey, Error> {
    match addr {
        Address::Established(_) => {
            display_line!(
                IO,
                "Looking-up public key of {} from the ledger...",
                addr.encode()
            );
            rpc::get_public_key_at(client, addr, 0)
                .await?
                .ok_or(Error::Other(format!(
                    "No public key found for the address {}",
                    addr.encode()
                )))
        }
        Address::Implicit(ImplicitAddress(pkh)) => Ok(wallet
            .find_key_by_pkh(pkh, password)
            .map_err(|err| {
                Error::Other(format!(
                    "Unable to load the keypair from the wallet for the \
                     implicit address {}. Failed with: {}",
                    addr.encode(),
                    err
                ))
            })?
            .ref_to()),
        Address::Internal(_) => other_err(format!(
            "Internal address {} doesn't have any signing keys.",
            addr
        )),
    }
}

/// Load the secret key corresponding to the given public key from the wallet.
/// If the keypair is encrypted but a password is not supplied, then it is
/// interactively prompted. Errors if the key cannot be found or loaded.
pub fn find_key_by_pk<U: WalletUtils>(
    wallet: &mut Wallet<U>,
    args: &args::Tx,
    public_key: &common::PublicKey,
) -> Result<common::SecretKey, Error> {
    if *public_key == masp_tx_key().ref_to() {
        // We already know the secret key corresponding to the MASP sentinal key
        Ok(masp_tx_key())
    } else {
        // Otherwise we need to search the wallet for the secret key
        wallet
            .find_key_by_pk(public_key, args.password.clone())
            .map_err(|err| {
                Error::Other(format!(
                    "Unable to load the keypair from the wallet for public \
                     key {}. Failed with: {}",
                    public_key, err
                ))
            })
    }
}

/// Given CLI arguments and some defaults, determine the rightful transaction
/// signer. Return the given signing key or public key of the given signer if
/// possible. If no explicit signer given, use the `default`. If no `default`
/// is given, an `Error` is returned.
pub async fn tx_signers<
    C: crate::sdk::queries::Client + Sync,
    U: WalletUtils,
    IO: Io,
>(
    client: &C,
    wallet: &mut Wallet<U>,
    args: &args::Tx,
    default: Option<Address>,
) -> Result<Vec<common::PublicKey>, Error> {
    let signer = if !&args.signing_keys.is_empty() {
        let public_keys =
            args.signing_keys.iter().map(|key| key.ref_to()).collect();
        return Ok(public_keys);
    } else if let Some(verification_key) = &args.verification_key {
        return Ok(vec![verification_key.clone()]);
    } else {
        // Otherwise use the signer determined by the caller
        default
    };

    // Now actually fetch the signing key and apply it
    match signer {
        Some(signer) if signer == masp() => Ok(vec![masp_tx_key().ref_to()]),

        Some(signer) => Ok(vec![
            find_pk::<C, U, IO>(client, wallet, &signer, args.password.clone())
                .await?,
        ]),
        None => other_err(
            "All transactions must be signed; please either specify the key \
             or the address from which to look up the signing key."
                .to_string(),
        ),
    }
}

/// Sign a transaction with a given signing key or public key of a given signer.
/// If no explicit signer given, use the `default`. If no `default` is given,
/// Error.
///
/// It also takes a second, optional keypair to sign the wrapper header
/// separately.
///
/// If this is not a dry run, the tx is put in a wrapper and returned along with
/// hashes needed for monitoring the tx on chain.
///
/// If it is a dry run, it is not put in a wrapper, but returned as is.
pub fn sign_tx<U: WalletUtils>(
    wallet: &mut Wallet<U>,
    args: &args::Tx,
    tx: &mut Tx,
    signing_data: SigningTxData,
) -> Result<(), Error> {
    if !args.signatures.is_empty() {
        let signatures = args
            .signatures
            .iter()
            .map(|bytes| SignatureIndex::deserialize(bytes).unwrap())
            .collect();
        tx.add_signatures(signatures);
    } else if let Some(account_public_keys_map) =
        signing_data.account_public_keys_map
    {
        let signing_tx_keypairs = signing_data
            .public_keys
            .iter()
            .filter_map(|public_key| {
                match find_key_by_pk(wallet, args, public_key) {
                    Ok(secret_key) => Some(secret_key),
                    Err(_) => None,
                }
            })
            .collect::<Vec<common::SecretKey>>();
        tx.sign_raw(
            signing_tx_keypairs,
            account_public_keys_map,
            signing_data.owner,
        );
    }

    let fee_payer_keypair =
        find_key_by_pk(wallet, args, &signing_data.fee_payer)?;
    tx.sign_wrapper(fee_payer_keypair);
    Ok(())
}

/// Return the necessary data regarding an account to be able to generate a
/// multisignature section
pub async fn aux_signing_data<
    C: crate::sdk::queries::Client + Sync,
    U: WalletUtils,
    IO: Io,
>(
    client: &C,
    wallet: &mut Wallet<U>,
    args: &args::Tx,
    owner: Option<Address>,
    default_signer: Option<Address>,
) -> Result<SigningTxData, Error> {
    let public_keys = if owner.is_some() || args.wrapper_fee_payer.is_none() {
        tx_signers::<C, U, IO>(client, wallet, args, default_signer.clone())
            .await?
    } else {
        vec![]
    };

    let (account_public_keys_map, threshold) = match &owner {
        Some(owner @ Address::Established(_)) => {
            let account = rpc::get_account_info::<C>(client, owner).await?;
            if let Some(account) = account {
                (Some(account.public_keys_map), account.threshold)
            } else {
                return Err(Error::from(TxError::InvalidAccount(
                    owner.encode(),
                )));
            }
        }
        Some(Address::Implicit(_)) => (
            Some(AccountPublicKeysMap::from_iter(public_keys.clone())),
            1u8,
        ),
        Some(owner @ Address::Internal(_)) => {
            return Err(Error::from(TxError::InvalidAccount(owner.encode())));
        }
        None => (None, 0u8),
    };

    let fee_payer = if args.disposable_signing_key {
        wallet.generate_disposable_signing_key().to_public()
    } else {
        match &args.wrapper_fee_payer {
            Some(keypair) => keypair.to_public(),
            None => public_keys.get(0).ok_or(TxError::InvalidFeePayer)?.clone(),
        }
    };

    if fee_payer == masp_tx_key().to_public() {
        other_err(
            "The gas payer cannot be the MASP, please provide a different gas \
             payer."
                .to_string(),
        )?;
    }

    Ok(SigningTxData {
        owner,
        public_keys,
        threshold,
        account_public_keys_map,
        fee_payer,
    })
}

/// Informations about the post-tx balance of the tx's source. Used to correctly
/// handle fee validation in the wrapper tx
pub struct TxSourcePostBalance {
    /// The balance of the tx source after the tx has been applied
    pub post_balance: Amount,
    /// The source address of the tx
    pub source: Address,
    /// The token of the tx
    pub token: Address,
}

/// Create a wrapper tx from a normal tx. Get the hash of the
/// wrapper and its payload which is needed for monitoring its
/// progress on chain.
#[allow(clippy::too_many_arguments)]
pub async fn wrap_tx<
    C: crate::sdk::queries::Client + Sync,
    V: ShieldedUtils,
    IO: Io,
>(
    client: &C,
    shielded: &mut ShieldedContext<V>,
    tx: &mut Tx,
    args: &args::Tx,
    tx_source_balance: Option<TxSourcePostBalance>,
    epoch: Epoch,
    fee_payer: common::PublicKey,
) -> Result<Option<Epoch>, Error> {
    let fee_payer_address = Address::from(&fee_payer);
    // Validate fee amount and token
    let gas_cost_key = parameter_storage::get_gas_cost_key();
    let minimum_fee = match rpc::query_storage_value::<
        C,
        BTreeMap<Address, Amount>,
    >(client, &gas_cost_key)
    .await
    .and_then(|map| {
        map.get(&args.fee_token)
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                Error::Other(format!(
                    "Could not retrieve from storage the gas cost for token {}",
                    args.fee_token
                ))
            })
    }) {
        Ok(amount) => amount,
        Err(e) => {
            if !args.force {
                return Err(e);
            } else {
                token::Amount::default()
            }
        }
    };
    let fee_amount = match args.fee_amount {
        Some(amount) => {
            let validated_fee_amount = validate_amount::<_, IO>(
                client,
                amount,
                &args.fee_token,
                args.force,
            )
            .await
            .expect("Expected to be able to validate fee");

            let amount =
                Amount::from_uint(validated_fee_amount.amount, 0).unwrap();

            if amount >= minimum_fee {
                amount
            } else if !args.force {
                // Update the fee amount if it's not enough
                display_line!(
                    IO,
                    "The provided gas price {} is less than the minimum \
                     amount required {}, changing it to match the minimum",
                    amount.to_string_native(),
                    minimum_fee.to_string_native()
                );
                minimum_fee
            } else {
                amount
            }
        }
        None => minimum_fee,
    };

    let mut updated_balance = match tx_source_balance {
        Some(TxSourcePostBalance {
            post_balance: balance,
            source,
            token,
        }) if token == args.fee_token && source == fee_payer_address => balance,
        _ => {
            let balance_key =
                token::balance_key(&args.fee_token, &fee_payer_address);

            rpc::query_storage_value::<C, token::Amount>(client, &balance_key)
                .await
                .unwrap_or_default()
        }
    };

    let total_fee = fee_amount * u64::from(args.gas_limit);

    let (unshield, unshielding_epoch) = match total_fee
        .checked_sub(updated_balance)
    {
        Some(diff) if !diff.is_zero() => {
            if let Some(spending_key) = args.fee_unshield.clone() {
                // Unshield funds for fee payment
                let transfer_args = args::TxTransfer {
                    tx: args.to_owned(),
                    source: spending_key,
                    target: namada_core::types::masp::TransferTarget::Address(
                        fee_payer_address.clone(),
                    ),
                    token: args.fee_token.clone(),
                    amount: args::InputAmount::Validated(DenominatedAmount {
                        // NOTE: must unshield the total fee amount, not the
                        // diff, because the ledger evaluates the transaction in
                        // reverse (wrapper first, inner second) and cannot know
                        // ahead of time if the inner will modify the balance of
                        // the gas payer
                        amount: total_fee,
                        denom: 0.into(),
                    }),
                    // These last two fields are not used in the function, mock
                    // them
                    native_token: args.fee_token.clone(),
                    tx_code_path: PathBuf::new(),
                };

                match shielded
                    .gen_shielded_transfer::<_, IO>(client, transfer_args)
                    .await
                {
                    Ok(Some(ShieldedTransfer {
                        builder: _,
                        masp_tx: transaction,
                        metadata: _data,
                        epoch: unshielding_epoch,
                    })) => {
                        let spends = transaction
                            .sapling_bundle()
                            .unwrap()
                            .shielded_spends
                            .len();
                        let converts = transaction
                            .sapling_bundle()
                            .unwrap()
                            .shielded_converts
                            .len();
                        let outs = transaction
                            .sapling_bundle()
                            .unwrap()
                            .shielded_outputs
                            .len();

                        let descriptions = spends + converts + outs;

                        let descriptions_limit_key=  parameter_storage::get_fee_unshielding_descriptions_limit_key();
                        let descriptions_limit =
                            rpc::query_storage_value::<C, u64>(
                                client,
                                &descriptions_limit_key,
                            )
                            .await
                            .unwrap();

                        if u64::try_from(descriptions).unwrap()
                            > descriptions_limit
                            && !args.force
                        {
                            return Err(Error::from(
                                TxError::FeeUnshieldingError(format!(
                                    "Descriptions exceed the limit: found \
                                     {descriptions}, limit \
                                     {descriptions_limit}"
                                )),
                            ));
                        }

                        updated_balance += total_fee;
                        (Some(transaction), Some(unshielding_epoch))
                    }
                    Ok(None) => {
                        if !args.force {
                            return Err(Error::from(
                                TxError::FeeUnshieldingError(
                                    "Missing unshielding transaction"
                                        .to_string(),
                                ),
                            ));
                        }

                        (None, None)
                    }
                    Err(e) => {
                        if !args.force {
                            return Err(Error::from(
                                TxError::FeeUnshieldingError(e.to_string()),
                            ));
                        }

                        (None, None)
                    }
                }
            } else {
                let token_addr = args.fee_token.clone();
                if !args.force {
                    let fee_amount = format_denominated_amount::<_, IO>(
                        client,
                        &token_addr,
                        total_fee,
                    )
                    .await;

                    let balance = format_denominated_amount::<_, IO>(
                        client,
                        &token_addr,
                        updated_balance,
                    )
                    .await;
                    return Err(Error::from(TxError::BalanceTooLowForFees(
                        fee_payer_address,
                        token_addr,
                        fee_amount,
                        balance,
                    )));
                }

                (None, None)
            }
        }
        _ => {
            if args.fee_unshield.is_some() {
                display_line!(
                    IO,
                    "Enough transparent balance to pay fees: the fee \
                     unshielding spending key will be ignored"
                );
            }
            (None, None)
        }
    };

    let unshield_section_hash = unshield.map(|masp_tx| {
        let section = Section::MaspTx(masp_tx);
        let mut hasher = sha2::Sha256::new();
        section.hash(&mut hasher);
        tx.add_section(section);
        namada_core::types::hash::Hash(hasher.finalize().into())
    });

    tx.add_wrapper(
        Fee {
            amount_per_gas_unit: fee_amount,
            token: args.fee_token.clone(),
        },
        fee_payer,
        epoch,
        // TODO: partially validate the gas limit in client
        args.gas_limit,
        unshield_section_hash,
    );

    Ok(unshielding_epoch)
}

#[allow(clippy::result_large_err)]
fn other_err<T>(string: String) -> Result<T, Error> {
    Err(Error::Other(string))
}

/// Represents the transaction data that is displayed on a Ledger device
#[derive(Default, Serialize, Deserialize)]
pub struct LedgerVector {
    blob: String,
    index: u64,
    name: String,
    output: Vec<String>,
    output_expert: Vec<String>,
    valid: bool,
}

/// Adds a Ledger output line describing a given transaction amount and address
fn make_ledger_amount_addr(
    tokens: &HashMap<Address, String>,
    output: &mut Vec<String>,
    amount: DenominatedAmount,
    token: &Address,
    prefix: &str,
) {
    if let Some(token) = tokens.get(token) {
        output.push(format!(
            "{}Amount : {} {}",
            prefix,
            token.to_uppercase(),
            to_ledger_decimal(&amount.to_string()),
        ));
    } else {
        output.extend(vec![
            format!("{}Token : {}", prefix, token),
            format!(
                "{}Amount : {}",
                prefix,
                to_ledger_decimal(&amount.to_string())
            ),
        ]);
    }
}

/// Adds a Ledger output line describing a given transaction amount and asset
/// type
async fn make_ledger_amount_asset<
    C: crate::ledger::queries::Client + Sync,
    IO: Io,
>(
    client: &C,
    tokens: &HashMap<Address, String>,
    output: &mut Vec<String>,
    amount: u64,
    token: &AssetType,
    assets: &HashMap<AssetType, (Address, MaspDenom, Epoch)>,
    prefix: &str,
) {
    if let Some((token, _, _epoch)) = assets.get(token) {
        // If the AssetType can be decoded, then at least display Addressees
        let formatted_amt =
            format_denominated_amount::<_, IO>(client, token, amount.into())
                .await;
        if let Some(token) = tokens.get(token) {
            output.push(format!(
                "{}Amount : {} {}",
                prefix,
                token.to_uppercase(),
                to_ledger_decimal(&formatted_amt),
            ));
        } else {
            output.extend(vec![
                format!("{}Token : {}", prefix, token),
                format!(
                    "{}Amount : {}",
                    prefix,
                    to_ledger_decimal(&formatted_amt)
                ),
            ]);
        }
    } else {
        // Otherwise display the raw AssetTypes
        output.extend(vec![
            format!("{}Token : {}", prefix, token),
            format!(
                "{}Amount : {}",
                prefix,
                to_ledger_decimal(&amount.to_string())
            ),
        ]);
    }
}

/// Split the lines in the vector that are longer than the Ledger device's
/// character width
fn format_outputs(output: &mut Vec<String>) {
    const MAX_KEY_LEN: usize = 39;
    const MAX_VALUE_LEN: usize = 39;

    let mut i = 0;
    let mut pos = 0;
    // Break down each line that is too long one-by-one
    while pos < output.len() {
        let curr_line = output[pos].clone();
        let (key, mut value) =
            curr_line.split_once(':').unwrap_or(("", &curr_line));
        // Truncate the key length to the declared maximum
        let key = key.trim().chars().take(MAX_KEY_LEN - 1).collect::<String>();
        // Trim value because we will insert spaces later
        value = value.trim();
        if value.is_empty() {
            value = "(none)"
        }
        if value.chars().count() < MAX_VALUE_LEN {
            // No need to split the line in this case
            output[pos] = format!("{} | {} : {}", i, key, value);
            pos += 1;
        } else {
            // Line is too long so split it up. Repeat the key on each line
            output.remove(pos);
            let part_count = (value.chars().count() + MAX_VALUE_LEN - 2)
                / (MAX_VALUE_LEN - 1);
            for (idx, part) in value
                .chars()
                .chunks(MAX_VALUE_LEN - 1)
                .into_iter()
                .enumerate()
            {
                let line = format!(
                    "{} | {} [{}/{}] : {}",
                    i,
                    key,
                    idx + 1,
                    part_count,
                    part.collect::<String>(),
                );
                output.insert(pos, line);
                pos += 1;
            }
        }
        i += 1;
    }
}

/// Adds a Ledger output for the sender and destination for transparent and MASP
/// transactions
pub async fn make_ledger_masp_endpoints<
    C: crate::ledger::queries::Client + Sync,
    IO: Io,
>(
    client: &C,
    tokens: &HashMap<Address, String>,
    output: &mut Vec<String>,
    transfer: &Transfer,
    builder: Option<&MaspBuilder>,
    assets: &HashMap<AssetType, (Address, MaspDenom, Epoch)>,
) {
    if transfer.source != masp() {
        output.push(format!("Sender : {}", transfer.source));
        if transfer.target == masp() {
            make_ledger_amount_addr(
                tokens,
                output,
                transfer.amount,
                &transfer.token,
                "Sending ",
            );
        }
    } else if let Some(builder) = builder {
        for sapling_input in builder.builder.sapling_inputs() {
            let vk = ExtendedViewingKey::from(*sapling_input.key());
            output.push(format!("Sender : {}", vk));
            make_ledger_amount_asset::<_, IO>(
                client,
                tokens,
                output,
                sapling_input.value(),
                &sapling_input.asset_type(),
                assets,
                "Sending ",
            )
            .await;
        }
    }
    if transfer.target != masp() {
        output.push(format!("Destination : {}", transfer.target));
        if transfer.source == masp() {
            make_ledger_amount_addr(
                tokens,
                output,
                transfer.amount,
                &transfer.token,
                "Receiving ",
            );
        }
    } else if let Some(builder) = builder {
        for sapling_output in builder.builder.sapling_outputs() {
            let pa = PaymentAddress::from(sapling_output.address());
            output.push(format!("Destination : {}", pa));
            make_ledger_amount_asset::<_, IO>(
                client,
                tokens,
                output,
                sapling_output.value(),
                &sapling_output.asset_type(),
                assets,
                "Receiving ",
            )
            .await;
        }
    }
    if transfer.source != masp() && transfer.target != masp() {
        make_ledger_amount_addr(
            tokens,
            output,
            transfer.amount,
            &transfer.token,
            "",
        );
    }
}

/// Internal method used to generate transaction test vectors
#[cfg(feature = "std")]
pub async fn generate_test_vector<
    C: crate::sdk::queries::Client + Sync,
    U: WalletUtils,
    IO: Io,
>(
    client: &C,
    wallet: &mut Wallet<U>,
    tx: &Tx,
) -> Result<(), Error> {
    use std::env;
    use std::fs::File;
    use std::io::Write;

    if let Ok(path) = env::var(ENV_VAR_LEDGER_LOG_PATH) {
        let mut tx = tx.clone();
        // Contract the large data blobs in the transaction
        tx.wallet_filter();
        // Convert the transaction to Ledger format
        let decoding =
            to_ledger_vector::<_, _, IO>(client, wallet, &tx).await?;
        let output = serde_json::to_string(&decoding)
            .map_err(|e| Error::from(EncodingError::Serde(e.to_string())))?;
        // Record the transaction at the identified path
        let mut f = File::options()
            .append(true)
            .create(true)
            .open(path)
            .map_err(|e| {
                Error::Other(format!("failed to open test vector file: {}", e))
            })?;
        writeln!(f, "{},", output).map_err(|_| {
            Error::Other("unable to write test vector to file".to_string())
        })?;
    }

    // Attempt to decode the construction
    if let Ok(path) = env::var(ENV_VAR_TX_LOG_PATH) {
        let mut tx = tx.clone();
        // Contract the large data blobs in the transaction
        tx.wallet_filter();
        // Record the transaction at the identified path
        let mut f = File::options()
            .append(true)
            .create(true)
            .open(path)
            .map_err(|_| {
                Error::Other("unable to write test vector to file".to_string())
            })?;
        writeln!(f, "{:x?},", tx).map_err(|_| {
            Error::Other("unable to write test vector to file".to_string())
        })?;
    }
    Ok(())
}

/// Convert decimal numbers into the format used by Ledger. Specifically remove
/// all insignificant zeros occuring after decimal point.
fn to_ledger_decimal(amount: &str) -> String {
    if amount.contains('.') {
        let mut amount = amount.trim_end_matches('0').to_string();
        if amount.ends_with('.') {
            amount.push('0')
        }
        amount
    } else {
        amount.to_string() + ".0"
    }
}

/// A ProposalVote wrapper that prints the spending cap with Ledger decimal
/// formatting.
struct LedgerProposalVote<'a>(&'a StorageProposalVote);

impl<'a> Display for LedgerProposalVote<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            StorageProposalVote::Yay(vote_type) => match vote_type {
                VoteType::Default => write!(f, "yay"),
                VoteType::PGFSteward => write!(f, "yay for PGF steward"),
                VoteType::PGFPayment => {
                    write!(f, "yay for PGF payment proposal")
                }
            },

            StorageProposalVote::Nay => write!(f, "nay"),
        }
    }
}

/// A ProposalType wrapper that prints the hash of the contained WASM code if it
/// is present.
struct LedgerProposalType<'a>(&'a ProposalType, &'a Tx);

impl<'a> Display for LedgerProposalType<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.0 {
            ProposalType::Default(None) => write!(f, "Default"),
            ProposalType::Default(Some(hash)) => {
                let extra = self
                    .1
                    .get_section(hash)
                    .and_then(|x| Section::extra_data_sec(x.as_ref()))
                    .expect("unable to load vp code")
                    .code
                    .hash();
                write!(f, "{}", HEXLOWER.encode(&extra.0))
            }
            ProposalType::PGFSteward(_) => write!(f, "PGF Steward"),
            ProposalType::PGFPayment(_) => write!(f, "PGF Payment"),
        }
    }
}

/// Converts the given transaction to the form that is displayed on the Ledger
/// device
pub async fn to_ledger_vector<
    C: crate::sdk::queries::Client + Sync,
    U: WalletUtils,
    IO: Io,
>(
    client: &C,
    wallet: &mut Wallet<U>,
    tx: &Tx,
) -> Result<LedgerVector, Error> {
    let init_account_hash =
        query_wasm_code_hash::<_, IO>(client, TX_INIT_ACCOUNT_WASM).await?;
    let init_validator_hash =
        query_wasm_code_hash::<_, IO>(client, TX_INIT_VALIDATOR_WASM).await?;
    let init_proposal_hash =
        query_wasm_code_hash::<_, IO>(client, TX_INIT_PROPOSAL).await?;
    let vote_proposal_hash =
        query_wasm_code_hash::<_, IO>(client, TX_VOTE_PROPOSAL).await?;
    let reveal_pk_hash =
        query_wasm_code_hash::<_, IO>(client, TX_REVEAL_PK).await?;
    let update_account_hash =
        query_wasm_code_hash::<_, IO>(client, TX_UPDATE_ACCOUNT_WASM).await?;
    let transfer_hash =
        query_wasm_code_hash::<_, IO>(client, TX_TRANSFER_WASM).await?;
    let ibc_hash = query_wasm_code_hash::<_, IO>(client, TX_IBC_WASM).await?;
    let bond_hash = query_wasm_code_hash::<_, IO>(client, TX_BOND_WASM).await?;
    let unbond_hash =
        query_wasm_code_hash::<_, IO>(client, TX_UNBOND_WASM).await?;
    let withdraw_hash =
        query_wasm_code_hash::<_, IO>(client, TX_WITHDRAW_WASM).await?;
    let change_commission_hash =
        query_wasm_code_hash::<_, IO>(client, TX_CHANGE_COMMISSION_WASM)
            .await?;
    let user_hash = query_wasm_code_hash::<_, IO>(client, VP_USER_WASM).await?;
    let unjail_validator_hash =
        query_wasm_code_hash::<_, IO>(client, TX_UNJAIL_VALIDATOR_WASM).await?;

    // To facilitate lookups of human-readable token names
    let tokens: HashMap<Address, String> = wallet
        .get_addresses()
        .into_iter()
        .map(|(alias, addr)| (addr, alias))
        .collect();

    let mut tv = LedgerVector {
        blob: HEXLOWER.encode(&tx.try_to_vec().map_err(|_| {
            Error::Other("unable to serialize transaction".to_string())
        })?),
        index: 0,
        valid: true,
        name: "Custom_0".to_string(),
        ..Default::default()
    };

    let code_hash = tx
        .get_section(tx.code_sechash())
        .ok_or_else(|| {
            Error::Other("expected tx code section to be present".to_string())
        })?
        .code_sec()
        .ok_or_else(|| {
            Error::Other("expected section to have code tag".to_string())
        })?
        .code
        .hash();
    tv.output_expert
        .push(format!("Code hash : {}", HEXLOWER.encode(&code_hash.0)));

    if code_hash == init_account_hash {
        let init_account = InitAccount::try_from_slice(
            &tx.data()
                .ok_or_else(|| Error::Other("Invalid Data".to_string()))?,
        )
        .map_err(|err| {
            Error::from(EncodingError::Conversion(err.to_string()))
        })?;
        tv.name = "Init_Account_0".to_string();

        let extra = tx
            .get_section(&init_account.vp_code_hash)
            .and_then(|x| Section::extra_data_sec(x.as_ref()))
            .ok_or_else(|| Error::Other("unable to load vp code".to_string()))?
            .code
            .hash();
        let vp_code = if extra == user_hash {
            "User".to_string()
        } else {
            HEXLOWER.encode(&extra.0)
        };
        tv.output.extend(vec![format!("Type : Init Account")]);
        tv.output.extend(
            init_account
                .public_keys
                .iter()
                .map(|k| format!("Public key : {}", k.to_string())),
        );
        tv.output.extend(vec![
            format!("Threshold : {}", init_account.threshold),
            format!("VP type : {}", vp_code),
        ]);

        tv.output_expert.extend(
            init_account
                .public_keys
                .iter()
                .map(|k| format!("Public key : {}", k.to_string())),
        );
        tv.output_expert.extend(vec![
            format!("Threshold : {}", init_account.threshold),
            format!("VP type : {}", HEXLOWER.encode(&extra.0)),
        ]);
    } else if code_hash == init_validator_hash {
        let init_validator = InitValidator::try_from_slice(
            &tx.data()
                .ok_or_else(|| Error::Other("Invalid Data".to_string()))?,
        )
        .map_err(|err| {
            Error::from(EncodingError::Conversion(err.to_string()))
        })?;

        tv.name = "Init_Validator_0".to_string();

        let extra = tx
            .get_section(&init_validator.validator_vp_code_hash)
            .and_then(|x| Section::extra_data_sec(x.as_ref()))
            .ok_or_else(|| Error::Other("unable to load vp code".to_string()))?
            .code
            .hash();
        let vp_code = if extra == user_hash {
            "User".to_string()
        } else {
            HEXLOWER.encode(&extra.0)
        };

        tv.output.extend(vec!["Type : Init Validator".to_string()]);
        tv.output.extend(
            init_validator
                .account_keys
                .iter()
                .map(|k| format!("Account key : {}", k.to_string())),
        );
        tv.output.extend(vec![
            format!("Threshold : {}", init_validator.threshold),
            format!("Consensus key : {}", init_validator.consensus_key),
            format!("Ethereum cold key : {}", init_validator.eth_cold_key),
            format!("Ethereum hot key : {}", init_validator.eth_hot_key),
            format!("Protocol key : {}", init_validator.protocol_key),
            format!("DKG key : {}", init_validator.dkg_key),
            format!("Commission rate : {}", init_validator.commission_rate),
            format!(
                "Maximum commission rate change : {}",
                init_validator.max_commission_rate_change
            ),
            format!("Validator VP type : {}", vp_code,),
        ]);

        tv.output_expert.extend(
            init_validator
                .account_keys
                .iter()
                .map(|k| format!("Account key : {}", k.to_string())),
        );
        tv.output_expert.extend(vec![
            format!("Threshold : {}", init_validator.threshold),
            format!("Consensus key : {}", init_validator.consensus_key),
            format!("Ethereum cold key : {}", init_validator.eth_cold_key),
            format!("Ethereum hot key : {}", init_validator.eth_hot_key),
            format!("Protocol key : {}", init_validator.protocol_key),
            format!("DKG key : {}", init_validator.dkg_key),
            format!("Commission rate : {}", init_validator.commission_rate),
            format!(
                "Maximum commission rate change : {}",
                init_validator.max_commission_rate_change
            ),
            format!("Validator VP type : {}", HEXLOWER.encode(&extra.0)),
        ]);
    } else if code_hash == init_proposal_hash {
        let init_proposal_data = InitProposalData::try_from_slice(
            &tx.data()
                .ok_or_else(|| Error::Other("Invalid Data".to_string()))?,
        )
        .map_err(|err| {
            Error::from(EncodingError::Conversion(err.to_string()))
        })?;

        tv.name = "Init_Proposal_0".to_string();

        let extra = tx
            .get_section(&init_proposal_data.content)
            .and_then(|x| Section::extra_data_sec(x.as_ref()))
            .expect("unable to load vp code")
            .code
            .hash();

        tv.output.push(format!("Type : Init proposal"));
        if let Some(id) = init_proposal_data.id.as_ref() {
            tv.output.push(format!("ID : {}", id));
        }
        tv.output.extend(vec![
            format!(
                "Proposal type : {}",
                LedgerProposalType(&init_proposal_data.r#type, tx)
            ),
            format!("Author : {}", init_proposal_data.author),
            format!(
                "Voting start epoch : {}",
                init_proposal_data.voting_start_epoch
            ),
            format!(
                "Voting end epoch : {}",
                init_proposal_data.voting_end_epoch
            ),
            format!("Grace epoch : {}", init_proposal_data.grace_epoch),
            format!("Content : {}", HEXLOWER.encode(&extra.0)),
        ]);

        if let Some(id) = init_proposal_data.id.as_ref() {
            tv.output_expert.push(format!("ID : {}", id));
        }
        tv.output_expert.extend(vec![
            format!(
                "Proposal type : {}",
                LedgerProposalType(&init_proposal_data.r#type, tx)
            ),
            format!("Author : {}", init_proposal_data.author),
            format!(
                "Voting start epoch : {}",
                init_proposal_data.voting_start_epoch
            ),
            format!(
                "Voting end epoch : {}",
                init_proposal_data.voting_end_epoch
            ),
            format!("Grace epoch : {}", init_proposal_data.grace_epoch),
            format!("Content : {}", HEXLOWER.encode(&extra.0)),
        ]);
    } else if code_hash == vote_proposal_hash {
        let vote_proposal = VoteProposalData::try_from_slice(
            &tx.data()
                .ok_or_else(|| Error::Other("Invalid Data".to_string()))?,
        )
        .map_err(|err| {
            Error::from(EncodingError::Conversion(err.to_string()))
        })?;

        tv.name = "Vote_Proposal_0".to_string();

        tv.output.extend(vec![
            format!("Type : Vote Proposal"),
            format!("ID : {}", vote_proposal.id),
            format!("Vote : {}", LedgerProposalVote(&vote_proposal.vote)),
            format!("Voter : {}", vote_proposal.voter),
        ]);
        for delegation in &vote_proposal.delegations {
            tv.output.push(format!("Delegation : {}", delegation));
        }

        tv.output_expert.extend(vec![
            format!("ID : {}", vote_proposal.id),
            format!("Vote : {}", LedgerProposalVote(&vote_proposal.vote)),
            format!("Voter : {}", vote_proposal.voter),
        ]);
        for delegation in vote_proposal.delegations {
            tv.output_expert
                .push(format!("Delegation : {}", delegation));
        }
    } else if code_hash == reveal_pk_hash {
        let public_key = common::PublicKey::try_from_slice(
            &tx.data()
                .ok_or_else(|| Error::Other("Invalid Data".to_string()))?,
        )
        .map_err(|err| {
            Error::from(EncodingError::Conversion(err.to_string()))
        })?;

        tv.name = "Reveal_Pubkey_0".to_string();

        tv.output.extend(vec![
            format!("Type : Reveal Pubkey"),
            format!("Public key : {}", public_key),
        ]);

        tv.output_expert
            .extend(vec![format!("Public key : {}", public_key)]);
    } else if code_hash == update_account_hash {
        let update_account = UpdateAccount::try_from_slice(
            &tx.data()
                .ok_or_else(|| Error::Other("Invalid Data".to_string()))?,
        )
        .map_err(|err| {
            Error::from(EncodingError::Conversion(err.to_string()))
        })?;

        tv.name = "Update_VP_0".to_string();

        match &update_account.vp_code_hash {
            Some(hash) => {
                let extra = tx
                    .get_section(hash)
                    .and_then(|x| Section::extra_data_sec(x.as_ref()))
                    .ok_or_else(|| {
                        Error::Other("unable to load vp code".to_string())
                    })?
                    .code
                    .hash();
                let vp_code = if extra == user_hash {
                    "User".to_string()
                } else {
                    HEXLOWER.encode(&extra.0)
                };
                tv.output.extend(vec![
                    format!("Type : Update VP"),
                    format!("Address : {}", update_account.addr),
                ]);
                tv.output.extend(
                    update_account
                        .public_keys
                        .iter()
                        .map(|k| format!("Public key : {}", k.to_string())),
                );
                if update_account.threshold.is_some() {
                    tv.output.extend(vec![format!(
                        "Threshold : {}",
                        update_account.threshold.unwrap()
                    )])
                }
                tv.output.extend(vec![format!("VP type : {}", vp_code)]);

                tv.output_expert
                    .extend(vec![format!("Address : {}", update_account.addr)]);
                tv.output_expert.extend(
                    update_account
                        .public_keys
                        .iter()
                        .map(|k| format!("Public key : {}", k.to_string())),
                );
                if update_account.threshold.is_some() {
                    tv.output_expert.extend(vec![format!(
                        "Threshold : {}",
                        update_account.threshold.unwrap()
                    )])
                }
                tv.output_expert.extend(vec![format!(
                    "VP type : {}",
                    HEXLOWER.encode(&extra.0)
                )]);
            }
            None => (),
        };
    } else if code_hash == transfer_hash {
        let transfer = Transfer::try_from_slice(
            &tx.data()
                .ok_or_else(|| Error::Other("Invalid Data".to_string()))?,
        )
        .map_err(|err| {
            Error::from(EncodingError::Conversion(err.to_string()))
        })?;
        // To facilitate lookups of MASP AssetTypes
        let mut asset_types = HashMap::new();
        let builder = if let Some(shielded_hash) = transfer.shielded {
            tx.sections.iter().find_map(|x| match x {
                Section::MaspBuilder(builder)
                    if builder.target == shielded_hash =>
                {
                    for (addr, denom, epoch) in &builder.asset_types {
                        match make_asset_type(Some(*epoch), addr, *denom) {
                            Err(_) => None,
                            Ok(asset) => {
                                asset_types.insert(
                                    asset,
                                    (addr.clone(), *denom, *epoch),
                                );
                                Some(builder)
                            }
                        }?;
                    }
                    Some(builder)
                }
                _ => None,
            })
        } else {
            None
        };

        tv.name = "Transfer_0".to_string();

        tv.output.push("Type : Transfer".to_string());
        make_ledger_masp_endpoints::<_, IO>(
            client,
            &tokens,
            &mut tv.output,
            &transfer,
            builder,
            &asset_types,
        )
        .await;
        make_ledger_masp_endpoints::<_, IO>(
            client,
            &tokens,
            &mut tv.output_expert,
            &transfer,
            builder,
            &asset_types,
        )
        .await;
    } else if code_hash == ibc_hash {
        let any_msg = Any::decode(
            tx.data()
                .ok_or_else(|| Error::Other("Invalid Data".to_string()))?
                .as_ref(),
        )
        .map_err(|x| Error::from(EncodingError::Conversion(x.to_string())))?;

        tv.name = "IBC_0".to_string();
        tv.output.push("Type : IBC".to_string());

        match MsgTransfer::try_from(any_msg.clone()) {
            Ok(transfer) => {
                let transfer_token = format!(
                    "{} {}",
                    transfer.packet_data.token.amount,
                    transfer.packet_data.token.denom
                );
                tv.output.extend(vec![
                    format!("Source port : {}", transfer.port_id_on_a),
                    format!("Source channel : {}", transfer.chan_id_on_a),
                    format!("Token : {}", transfer_token),
                    format!("Sender : {}", transfer.packet_data.sender),
                    format!("Receiver : {}", transfer.packet_data.receiver),
                    format!(
                        "Timeout height : {}",
                        transfer.timeout_height_on_b
                    ),
                    format!(
                        "Timeout timestamp : {}",
                        transfer.timeout_timestamp_on_b
                    ),
                ]);
                tv.output_expert.extend(vec![
                    format!("Source port : {}", transfer.port_id_on_a),
                    format!("Source channel : {}", transfer.chan_id_on_a),
                    format!("Token : {}", transfer_token),
                    format!("Sender : {}", transfer.packet_data.sender),
                    format!("Receiver : {}", transfer.packet_data.receiver),
                    format!(
                        "Timeout height : {}",
                        transfer.timeout_height_on_b
                    ),
                    format!(
                        "Timeout timestamp : {}",
                        transfer.timeout_timestamp_on_b
                    ),
                ]);
            }
            _ => {
                for line in format!("{:#?}", any_msg).split('\n') {
                    let stripped = line.trim_start();
                    tv.output.push(format!("Part : {}", stripped));
                    tv.output_expert.push(format!("Part : {}", stripped));
                }
            }
        }
    } else if code_hash == bond_hash {
        let bond = pos::Bond::try_from_slice(
            &tx.data()
                .ok_or_else(|| Error::Other("Invalid Data".to_string()))?,
        )
        .map_err(|err| {
            Error::from(EncodingError::Conversion(err.to_string()))
        })?;

        tv.name = "Bond_0".to_string();

        tv.output.push(format!("Type : Bond"));
        if let Some(source) = bond.source.as_ref() {
            tv.output.push(format!("Source : {}", source));
        }
        tv.output.extend(vec![
            format!("Validator : {}", bond.validator),
            format!(
                "Amount : NAM {}",
                to_ledger_decimal(&bond.amount.to_string_native())
            ),
        ]);

        if let Some(source) = bond.source.as_ref() {
            tv.output_expert.push(format!("Source : {}", source));
        }
        tv.output_expert.extend(vec![
            format!("Validator : {}", bond.validator),
            format!(
                "Amount : NAM {}",
                to_ledger_decimal(&bond.amount.to_string_native())
            ),
        ]);
    } else if code_hash == unbond_hash {
        let unbond = pos::Unbond::try_from_slice(
            &tx.data()
                .ok_or_else(|| Error::Other("Invalid Data".to_string()))?,
        )
        .map_err(|err| {
            Error::from(EncodingError::Conversion(err.to_string()))
        })?;

        tv.name = "Unbond_0".to_string();

        tv.output.push(format!("Type : Unbond"));
        if let Some(source) = unbond.source.as_ref() {
            tv.output.push(format!("Source : {}", source));
        }
        tv.output.extend(vec![
            format!("Validator : {}", unbond.validator),
            format!(
                "Amount : NAM {}",
                to_ledger_decimal(&unbond.amount.to_string_native())
            ),
        ]);

        if let Some(source) = unbond.source.as_ref() {
            tv.output_expert.push(format!("Source : {}", source));
        }
        tv.output_expert.extend(vec![
            format!("Validator : {}", unbond.validator),
            format!(
                "Amount : NAM {}",
                to_ledger_decimal(&unbond.amount.to_string_native())
            ),
        ]);
    } else if code_hash == withdraw_hash {
        let withdraw = pos::Withdraw::try_from_slice(
            &tx.data()
                .ok_or_else(|| Error::Other("Invalid Data".to_string()))?,
        )
        .map_err(|err| {
            Error::from(EncodingError::Conversion(err.to_string()))
        })?;

        tv.name = "Withdraw_0".to_string();

        tv.output.push(format!("Type : Withdraw"));
        if let Some(source) = withdraw.source.as_ref() {
            tv.output.push(format!("Source : {}", source));
        }
        tv.output
            .push(format!("Validator : {}", withdraw.validator));

        if let Some(source) = withdraw.source.as_ref() {
            tv.output_expert.push(format!("Source : {}", source));
        }
        tv.output_expert
            .push(format!("Validator : {}", withdraw.validator));
    } else if code_hash == change_commission_hash {
        let commission_change = pos::CommissionChange::try_from_slice(
            &tx.data()
                .ok_or_else(|| Error::Other("Invalid Data".to_string()))?,
        )
        .map_err(|err| {
            Error::from(EncodingError::Conversion(err.to_string()))
        })?;

        tv.name = "Change_Commission_0".to_string();

        tv.output.extend(vec![
            format!("Type : Change commission"),
            format!("New rate : {}", commission_change.new_rate),
            format!("Validator : {}", commission_change.validator),
        ]);

        tv.output_expert.extend(vec![
            format!("New rate : {}", commission_change.new_rate),
            format!("Validator : {}", commission_change.validator),
        ]);
    } else if code_hash == unjail_validator_hash {
        let address = Address::try_from_slice(
            &tx.data()
                .ok_or_else(|| Error::Other("Invalid Data".to_string()))?,
        )
        .map_err(|err| {
            Error::from(EncodingError::Conversion(err.to_string()))
        })?;

        tv.name = "Unjail_Validator_0".to_string();

        tv.output.extend(vec![
            format!("Type : Unjail Validator"),
            format!("Validator : {}", address),
        ]);

        tv.output_expert.push(format!("Validator : {}", address));
    } else {
        tv.name = "Custom_0".to_string();
        tv.output.push("Type : Custom".to_string());
    }

    if let Some(wrapper) = tx.header.wrapper() {
        let gas_token = wrapper.fee.token.clone();
        let gas_limit = format_denominated_amount::<_, IO>(
            client,
            &gas_token,
            Amount::from(wrapper.gas_limit),
        )
        .await;
        let fee_amount_per_gas_unit = format_denominated_amount::<_, IO>(
            client,
            &gas_token,
            wrapper.fee.amount_per_gas_unit,
        )
        .await;
        tv.output_expert.extend(vec![
            format!("Timestamp : {}", tx.header.timestamp.0),
            format!("Pubkey : {}", wrapper.pk),
            format!("Epoch : {}", wrapper.epoch),
            format!("Gas limit : {}", gas_limit),
        ]);
        if let Some(token) = tokens.get(&wrapper.fee.token) {
            tv.output_expert.push(format!(
                "Fees/gas unit : {} {}",
                token.to_uppercase(),
                to_ledger_decimal(&fee_amount_per_gas_unit),
            ));
        } else {
            tv.output_expert.extend(vec![
                format!("Fee token : {}", gas_token),
                format!("Fees/gas unit : {}", fee_amount_per_gas_unit),
            ]);
        }
    }

    // Finally, index each line and break those that are too long
    format_outputs(&mut tv.output);
    format_outputs(&mut tv.output_expert);
    Ok(tv)
}