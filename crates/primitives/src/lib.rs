//! Canonical development-protocol types. JSON is transport; BCS is consensus encoding.
use anyhow::{ensure, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::collections::BTreeMap;

pub type Hash = [u8; 32];
pub type Address = [u8; 32];
pub const VERSION: u32 = 1;
pub const MAX_BLOCK_BYTES: usize = 512 * 1024;
pub const MAX_TRANSACTIONS: usize = 256;
pub const MAX_ACCOUNTS: usize = 100_000;

pub fn hash<T: Serialize>(domain: &[u8], value: &T) -> Hash {
    let mut h = Sha3_256::new();
    h.update((domain.len() as u64).to_le_bytes());
    h.update(domain);
    // Only concrete protocol types with infallible BCS serialization are hashed.
    h.update(bcs::to_bytes(value).expect("protocol BCS serialization"));
    h.finalize().into()
}

pub fn address(public_key: &[u8; 32]) -> Address {
    hash(b"NEXIUM_ADDRESS_V1", public_key)
}

pub fn parse_hash(value: &str) -> Result<Hash> {
    let bytes = hex::decode(value.strip_prefix("0x").unwrap_or(value))?;
    ensure!(bytes.len() == 32, "expected 32-byte hex value");
    Ok(bytes.try_into().expect("checked length"))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Genesis {
    pub version: u32,
    pub network: String,
    pub timestamp_ms: u64,
    pub initial_target: u64,
    pub maximum_target: u64,
    pub target_spacing_ms: u64,
    pub retarget_interval: u64,
    pub block_reward: u64,
    pub reward_maturity: u64,
    pub allocations: Vec<(Address, u64)>,
}

impl Genesis {
    pub fn id(&self) -> Hash {
        hash(b"NEXIUM_GENESIS_V1", self)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == VERSION, "unsupported genesis version");
        ensure!(
            !self.network.is_empty() && self.network.len() <= 64,
            "invalid network name"
        );
        ensure!(
            self.initial_target > 0 && self.initial_target <= self.maximum_target,
            "invalid PoW target"
        );
        ensure!(self.maximum_target < u64::MAX, "target must require work");
        ensure!(
            (1..=60_000).contains(&self.target_spacing_ms),
            "invalid target spacing"
        );
        ensure!(
            (2..=1024).contains(&self.retarget_interval),
            "invalid retarget interval"
        );
        ensure!(
            self.reward_maturity > 0 && self.reward_maturity <= 1000,
            "invalid reward maturity"
        );
        ensure!(
            self.allocations.len() <= MAX_ACCOUNTS,
            "too many genesis accounts"
        );
        ensure!(
            self.allocations
                .windows(2)
                .all(|pair| pair[0].0 < pair[1].0),
            "genesis allocations must be sorted and unique"
        );
        let supply = self
            .allocations
            .iter()
            .try_fold(0u64, |sum, (_, n)| sum.checked_add(*n));
        ensure!(supply.is_some(), "genesis supply overflow");
        Ok(())
    }

    /// Public, intentionally insecure funding key for isolated development only.
    pub fn development() -> Self {
        let key = SigningKey::from_bytes(&[7; 32]);
        Self {
            version: VERSION,
            network: "nexium-local-v1".into(),
            timestamp_ms: 0,
            initial_target: u64::MAX / 256,
            maximum_target: u64::MAX / 16,
            target_spacing_ms: 10_000,
            retarget_interval: 20,
            block_reward: 50,
            reward_maturity: 10,
            allocations: vec![(address(&key.verifying_key().to_bytes()), 1_000_000_000)],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Transfer {
    pub network: Hash,
    pub public_key: [u8; 32],
    pub recipient: Address,
    pub amount: u64,
    pub fee: u64,
    pub nonce: u64,
    pub valid_until: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SignedTransfer {
    pub transfer: Transfer,
    pub signature: Vec<u8>,
}

impl SignedTransfer {
    pub fn sign(transfer: Transfer, key: &SigningKey) -> Result<Self> {
        ensure!(
            transfer.public_key == key.verifying_key().to_bytes(),
            "signing key mismatch"
        );
        let digest = hash(b"NEXIUM_TRANSFER_V1", &transfer);
        Ok(Self {
            transfer,
            signature: key.sign(&digest).to_bytes().to_vec(),
        })
    }
    pub fn id(&self) -> Hash {
        hash(b"NEXIUM_SIGNED_TRANSFER_V1", self)
    }
    pub fn sender(&self) -> Address {
        address(&self.transfer.public_key)
    }
    pub fn verify(&self, network: Hash) -> Result<()> {
        ensure!(self.transfer.network == network, "wrong network");
        ensure!(self.transfer.amount > 0, "zero transfer");
        let key = VerifyingKey::from_bytes(&self.transfer.public_key)?;
        let signature = Signature::from_slice(&self.signature)?;
        key.verify_strict(&hash(b"NEXIUM_TRANSFER_V1", &self.transfer), &signature)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub balance: u64,
    pub nonce: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reward {
    pub address: Address,
    pub amount: u64,
    pub unlock_height: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct State {
    pub accounts: BTreeMap<Address, Account>,
    pub immature_rewards: Vec<Reward>,
}

impl State {
    pub fn genesis(genesis: &Genesis) -> Self {
        Self {
            accounts: genesis
                .allocations
                .iter()
                .map(|(a, b)| {
                    (
                        *a,
                        Account {
                            balance: *b,
                            nonce: 0,
                        },
                    )
                })
                .collect(),
            immature_rewards: vec![],
        }
    }
    /// Full sorted-state commitment, deliberately not yet a Merkle proof tree.
    pub fn root(&self) -> Hash {
        hash(b"NEXIUM_STATE_V1", self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub version: u32,
    pub network: Hash,
    pub parent: Hash,
    pub height: u64,
    pub timestamp_ms: u64,
    pub target: u64,
    pub miner: Address,
    pub transactions_root: Hash,
    pub state_root: Hash,
    pub nonce: u64,
}

impl Header {
    pub fn id(&self) -> Hash {
        hash(b"NEXIUM_HEADER_V1", self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub header: Header,
    pub transactions: Vec<SignedTransfer>,
}

impl Block {
    pub fn id(&self) -> Hash {
        self.header.id()
    }
    pub fn transactions_root(transactions: &[SignedTransfer]) -> Hash {
        hash(b"NEXIUM_TRANSACTIONS_V1", &transactions)
    }
}
