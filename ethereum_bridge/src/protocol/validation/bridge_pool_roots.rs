//! Bridge pool roots validation.

use namada_core::ledger::storage;
use namada_core::ledger::storage::WlStorage;
use namada_core::proto::{SignableEthMessage, Signed};
use namada_core::types::keccak::keccak_hash;
use namada_core::types::storage::BlockHeight;
use namada_core::types::vote_extensions::bridge_pool_roots;
use namada_proof_of_stake::pos_queries::PosQueries;

use super::VoteExtensionError;
use crate::storage::eth_bridge_queries::EthBridgeQueries;

/// Validates a vote extension issued at the provided
/// block height signing over the latest Ethereum bridge
/// pool root and nonce.
///
/// Checks that at epoch of the provided height:
///  * The inner Namada address corresponds to a consensus validator.
///  * Check that the root and nonce are correct.
///  * The validator correctly signed the extension.
///  * The validator signed over the correct height inside of the extension.
///  * Check that the inner signature is valid.
pub fn validate_bp_roots_vext<D, H>(
    wl_storage: &WlStorage<D, H>,
    ext: &Signed<bridge_pool_roots::Vext>,
    last_height: BlockHeight,
) -> Result<(), VoteExtensionError>
where
    D: 'static + storage::DB + for<'iter> storage::DBIter<'iter>,
    H: 'static + storage::StorageHasher,
{
    // NOTE: for ABCI++, we should pass
    // `last_height` here, instead of `ext.data.block_height`
    let ext_height_epoch =
        match wl_storage.pos_queries().get_epoch(ext.data.block_height) {
            Some(epoch) => epoch,
            _ => {
                tracing::debug!(
                    block_height = ?ext.data.block_height,
                    "The epoch of the Bridge pool root's vote extension's \
                     block height should always be known",
                );
                return Err(VoteExtensionError::UnexpectedEpoch);
            }
        };
    if !wl_storage
        .ethbridge_queries()
        .is_bridge_active_at(ext_height_epoch)
    {
        tracing::debug!(
            vext_epoch = ?ext_height_epoch,
            "The Ethereum bridge was not enabled when the pool
             root's vote extension was cast",
        );
        return Err(VoteExtensionError::EthereumBridgeInactive);
    }

    if ext.data.block_height > last_height {
        tracing::debug!(
            ext_height = ?ext.data.block_height,
            ?last_height,
            "Bridge pool root's vote extension issued for a block height \
             higher than the chain's last height."
        );
        return Err(VoteExtensionError::UnexpectedBlockHeight);
    }
    if ext.data.block_height.0 == 0 {
        tracing::debug!("Dropping vote extension issued at genesis");
        return Err(VoteExtensionError::UnexpectedBlockHeight);
    }

    // get the public key associated with this validator
    let validator = &ext.data.validator_addr;
    let (_, pk) = wl_storage
        .pos_queries()
        .get_validator_from_address(validator, Some(ext_height_epoch))
        .map_err(|err| {
            tracing::debug!(
                ?err,
                %validator,
                "Could not get public key from Storage for some validator, \
                 while validating Bridge pool root's vote extension"
            );
            VoteExtensionError::PubKeyNotInStorage
        })?;
    // verify the signature of the vote extension
    ext.verify(&pk).map_err(|err| {
        tracing::debug!(
            ?err,
            ?ext.sig,
            ?pk,
            %validator,
            "Failed to verify the signature of an Bridge pool root's vote \
             extension issued by some validator"
        );
        VoteExtensionError::VerifySigFailed
    })?;

    let bp_root = wl_storage
        .ethbridge_queries()
        .get_bridge_pool_root_at_height(ext.data.block_height)
        .expect("We asserted that the queried height is correct")
        .0;
    let nonce = wl_storage
        .ethbridge_queries()
        .get_bridge_pool_nonce_at_height(ext.data.block_height)
        .to_bytes();
    let signed = Signed::<_, SignableEthMessage>::new_from(
        keccak_hash([bp_root, nonce].concat()),
        ext.data.sig.clone(),
    );
    let pk = wl_storage
        .pos_queries()
        .read_validator_eth_hot_key(validator, Some(ext_height_epoch))
        .expect("A validator should have an Ethereum hot key in storage.");
    signed.verify(&pk).map_err(|err| {
        tracing::debug!(
            ?err,
            ?signed.sig,
            ?pk,
            %validator,
            "Failed to verify the signature of an Bridge pool root \
            issued by some validator."
        );
        VoteExtensionError::InvalidBPRootSig
    })?;
    Ok(())
}
