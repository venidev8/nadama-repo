//! Storage keys for the Ethereum bridge account

pub mod storage;

use crate::types::address::{Address, InternalAddress};

/// The [`InternalAddress`] of the Ethereum bridge account
pub const INTERNAL_ADDRESS: InternalAddress = InternalAddress::EthBridge;

/// The [`Address`] of the Ethereum bridge account
pub const ADDRESS: Address = Address::Internal(INTERNAL_ADDRESS);
