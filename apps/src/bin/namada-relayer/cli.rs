//! Namada relayer CLI.

use color_eyre::eyre::Result;
use namada_apps::cli;
use namada_apps::cli::cmds;
use namada_apps::client::eth_bridge::{bridge_pool, validator_set};

pub async fn main() -> Result<()> {
    let (cmd, _) = cli::namada_relayer_cli()?;
    match cmd {
        cmds::NamadaRelayer::EthBridgePool(sub) => match sub {
            cmds::EthBridgePool::RecommendBatch(args) => {
                bridge_pool::recommend_batch(args).await;
            }
            cmds::EthBridgePool::ConstructProof(args) => {
                bridge_pool::construct_proof(args).await;
            }
            cmds::EthBridgePool::RelayProof(args) => {
                bridge_pool::relay_bridge_pool_proof(args).await;
            }
            cmds::EthBridgePool::QueryPool(query) => {
                bridge_pool::query_bridge_pool(query).await;
            }
            cmds::EthBridgePool::QuerySigned(query) => {
                bridge_pool::query_signed_bridge_pool(query).await;
            }
            cmds::EthBridgePool::QueryRelays(query) => {
                bridge_pool::query_relay_progress(query).await;
            }
        },
        cmds::NamadaRelayer::ValidatorSet(sub) => match sub {
            cmds::ValidatorSet::ConsensusValidatorSet(args) => {
                validator_set::query_validator_set_args(args).await;
            }
            cmds::ValidatorSet::ValidatorSetProof(args) => {
                validator_set::query_validator_set_update_proof(args).await;
            }
            cmds::ValidatorSet::ValidatorSetUpdateRelay(args) => {
                validator_set::relay_validator_set_update(args).await;
            }
        },
    }
    Ok(())
}