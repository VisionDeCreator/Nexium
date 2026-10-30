//! Development-only SHA3 PoW and sequential native-transfer execution.
use anyhow::{ensure, Context, Result};
use nexium_primitives::*;

pub const MAX_FUTURE_MS: u64 = 120_000;

pub fn work(target: u64) -> u128 {
    (1u128 << 64) / (u128::from(target) + 1)
}

pub fn valid_pow(header: &Header) -> bool {
    let prefix = u64::from_be_bytes(header.id()[..8].try_into().expect("hash prefix"));
    prefix <= header.target
}

/// Ancestors are oldest to newest and end at the prospective parent.
pub fn next_target(genesis: &Genesis, ancestors: &[Header], next_height: u64) -> Result<u64> {
    if next_height <= genesis.retarget_interval {
        return Ok(genesis.initial_target);
    }
    let parent = ancestors.last().context("missing difficulty parent")?;
    if !(next_height - 1).is_multiple_of(genesis.retarget_interval) {
        return Ok(parent.target);
    }
    let first = ancestors
        .iter()
        .find(|h| h.height == next_height - genesis.retarget_interval)
        .context("missing retarget ancestor")?;
    let expected =
        u128::from(genesis.target_spacing_ms) * u128::from(genesis.retarget_interval - 1);
    let elapsed = u128::from(
        parent
            .timestamp_ms
            .checked_sub(first.timestamp_ms)
            .context("timestamp underflow")?,
    );
    let elapsed = elapsed.clamp((expected / 4).max(1), expected * 4);
    Ok(
        (u128::from(parent.target) * elapsed / expected)
            .clamp(1, u128::from(genesis.maximum_target)) as u64,
    )
}

fn credit(state: &mut State, address: Address, amount: u64) -> Result<()> {
    if !state.accounts.contains_key(&address) {
        ensure!(
            state.accounts.len() < MAX_ACCOUNTS,
            "development account limit reached"
        );
    }
    let account = state.accounts.entry(address).or_default();
    account.balance = account
        .balance
        .checked_add(amount)
        .context("balance overflow")?;
    Ok(())
}

pub fn mature(state: &mut State, height: u64) -> Result<()> {
    let rewards = std::mem::take(&mut state.immature_rewards);
    for reward in rewards {
        if reward.unlock_height <= height {
            credit(state, reward.address, reward.amount)?;
        } else {
            state.immature_rewards.push(reward);
        }
    }
    Ok(())
}

/// Caller uses a temporary state; errors must never publish partial effects.
pub fn apply_transfer(
    state: &mut State,
    tx: &SignedTransfer,
    network: Hash,
    height: u64,
) -> Result<u64> {
    tx.verify(network)?;
    let transfer = &tx.transfer;
    ensure!(height <= transfer.valid_until, "transaction expired");
    let cost = transfer
        .amount
        .checked_add(transfer.fee)
        .context("transfer cost overflow")?;
    let sender = state
        .accounts
        .get_mut(&tx.sender())
        .context("sender does not exist")?;
    ensure!(sender.nonce == transfer.nonce, "invalid nonce");
    ensure!(sender.balance >= cost, "insufficient balance");
    sender.nonce = sender.nonce.checked_add(1).context("nonce overflow")?;
    sender.balance -= cost;
    credit(state, transfer.recipient, transfer.amount)?;
    Ok(transfer.fee)
}

pub fn execute(
    genesis: &Genesis,
    parent: &State,
    height: u64,
    miner: Address,
    transactions: &[SignedTransfer],
) -> Result<State> {
    ensure!(
        transactions.len() <= MAX_TRANSACTIONS,
        "too many block transactions"
    );
    let mut state = parent.clone();
    mature(&mut state, height)?;
    let mut reward = genesis.block_reward;
    let network = genesis.id();
    for tx in transactions {
        reward = reward
            .checked_add(apply_transfer(&mut state, tx, network, height)?)
            .context("reward overflow")?;
    }
    // Reserve account capacity now, so this valid reward can mature later.
    credit(&mut state, miner, 0)?;
    if reward > 0 {
        state.immature_rewards.push(Reward {
            address: miner,
            amount: reward,
            unlock_height: height
                .checked_add(genesis.reward_maturity)
                .context("height overflow")?,
        });
    }
    Ok(state)
}

pub fn validate(
    genesis: &Genesis,
    parent: &Header,
    parent_state: &State,
    ancestors: &[Header],
    block: &Block,
    now_ms: u64,
) -> Result<State> {
    ensure!(
        block.transactions.len() <= MAX_TRANSACTIONS,
        "too many transactions"
    );
    ensure!(
        bcs::to_bytes(block)?.len() <= MAX_BLOCK_BYTES,
        "block too large"
    );
    let h = &block.header;
    ensure!(
        h.version == VERSION && h.network == genesis.id(),
        "unsupported version or network"
    );
    ensure!(h.parent == parent.id(), "wrong parent");
    ensure!(
        h.height == parent.height.checked_add(1).context("height overflow")?,
        "wrong height"
    );
    ensure!(
        h.timestamp_ms > parent.timestamp_ms,
        "timestamp must increase"
    );
    ensure!(
        h.timestamp_ms <= now_ms.saturating_add(MAX_FUTURE_MS),
        "block too far in future; retry later"
    );
    ensure!(
        h.target == next_target(genesis, ancestors, h.height)?,
        "wrong difficulty"
    );
    ensure!(valid_pow(h), "invalid proof of work");
    ensure!(
        h.transactions_root == Block::transactions_root(&block.transactions),
        "transaction commitment mismatch"
    );
    let state = execute(
        genesis,
        parent_state,
        h.height,
        h.miner,
        &block.transactions,
    )?;
    ensure!(state.root() == h.state_root, "state commitment mismatch");
    Ok(state)
}

/// Mine in bounded chunks so callers can cancel stale work and shut down.
pub fn mine_chunk(header: &mut Header, attempts: u64) -> bool {
    for _ in 0..attempts {
        if valid_pow(header) {
            return true;
        }
        let Some(next) = header.nonce.checked_add(1) else {
            return false;
        };
        header.nonce = next;
    }
    false
}
