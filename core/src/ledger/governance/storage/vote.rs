use std::fmt::Display;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use super::super::cli::onchain::ProposalVote;

#[derive(
    Debug,
    Clone,
    PartialEq,
    BorshSerialize,
    BorshDeserialize,
    Eq,
    Serialize,
    Deserialize,
)]
/// The vote for a proposal
pub enum StorageProposalVote {
    /// Yes
    Yay,
    /// No
    Nay,
    /// Abstain
    Abstain,
}

impl StorageProposalVote {
    /// Check if a vote is yay
    pub fn is_yay(&self) -> bool {
        matches!(self, StorageProposalVote::Yay)
    }

    /// Check if a vote is nay
    pub fn is_nay(&self) -> bool {
        matches!(self, StorageProposalVote::Nay)
    }

    /// Check if a vote is abstain
    pub fn is_abstain(&self) -> bool {
        matches!(self, StorageProposalVote::Abstain)
    }
}

impl From<&ProposalVote> for StorageProposalVote {
    fn from(value: &ProposalVote) -> Self {
        match value {
            ProposalVote::Yay => StorageProposalVote::Yay,
            ProposalVote::Nay => StorageProposalVote::Nay,
            ProposalVote::Abstain => StorageProposalVote::Abstain,
        }
    }
}

impl Display for StorageProposalVote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageProposalVote::Yay => write!(f, "yay"),
            StorageProposalVote::Nay => write!(f, "nay"),
            StorageProposalVote::Abstain => write!(f, "abstain"),
        }
    }
}
