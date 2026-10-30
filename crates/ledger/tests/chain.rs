use ed25519_dalek::SigningKey;
use nexium_consensus::{execute, mine_chunk, next_target, valid_pow};
use nexium_ledger::{Ledger, SyncRequest, MAX_POOL_COUNT, SYNC_PAGE};
use nexium_primitives::*;
use tempfile::TempDir;

fn config() -> Genesis {
    let mut genesis = Genesis::development();
    genesis.initial_target = u64::MAX / 4;
    genesis.maximum_target = u64::MAX / 2;
    genesis.target_spacing_ms = 1000;
    genesis.retarget_interval = 4;
    genesis.reward_maturity = 2;
    genesis
}

fn open(temp: &TempDir, name: &str, genesis: &Genesis) -> Ledger {
    Ledger::open(&temp.path().join(name), genesis.clone()).unwrap()
}

fn seal(mut block: Block) -> Block {
    while !mine_chunk(&mut block.header, 1000) {}
    block
}

fn mine(ledger: &mut Ledger, miner: Address, timestamp: u64) -> Block {
    let block = seal(ledger.template(miner, timestamp).unwrap());
    assert!(ledger.import(block.clone(), timestamp).unwrap());
    block
}

fn transfer(genesis: &Genesis, nonce: u64, amount: u64) -> SignedTransfer {
    let key = SigningKey::from_bytes(&[7; 32]);
    SignedTransfer::sign(
        Transfer {
            network: genesis.id(),
            public_key: key.verifying_key().to_bytes(),
            recipient: [9; 32],
            amount,
            fee: 1,
            nonce,
            valid_until: 100,
        },
        &key,
    )
    .unwrap()
}

#[test]
fn signed_transfer_replay_protection_and_restart() {
    let temp = TempDir::new().unwrap();
    let genesis = config();
    let mut ledger = open(&temp, "node.sqlite", &genesis);
    let transaction = transfer(&genesis, 0, 123);
    let id = ledger.submit(transaction.clone()).unwrap();
    assert_eq!(ledger.submit(transaction.clone()).unwrap(), id);
    let block = mine(&mut ledger, [1; 32], 1000);
    assert_eq!(block.transactions, vec![transaction.clone()]);
    assert_eq!(ledger.account([9; 32]).unwrap().balance, 123);
    assert_eq!(ledger.status().unwrap().mempool_count, 0);
    assert!(ledger.submit(transaction).is_err());
    let expected = ledger.status().unwrap();
    drop(ledger);
    let ledger = open(&temp, "node.sqlite", &genesis);
    assert_eq!(ledger.status().unwrap().tip, expected.tip);
    assert_eq!(ledger.status().unwrap().state_root, expected.state_root);
    assert_eq!(ledger.account([9; 32]).unwrap().balance, 123);
    assert!(Ledger::open(&temp.path().join("node.sqlite"), genesis.clone()).is_err());
    drop(ledger);
    let mut other = genesis;
    other.network = "another-network".into();
    assert!(Ledger::open(&temp.path().join("node.sqlite"), other).is_err());
}

#[test]
fn invalid_transactions_leave_canonical_state_unchanged() {
    let temp = TempDir::new().unwrap();
    let genesis = config();
    let mut ledger = open(&temp, "node.sqlite", &genesis);
    let before = ledger.status().unwrap();
    let mut bad_signature = transfer(&genesis, 0, 10);
    bad_signature.signature[0] ^= 1;
    assert!(ledger.submit(bad_signature).is_err());
    assert!(ledger.submit(transfer(&genesis, 1, 10)).is_err());
    assert!(ledger.submit(transfer(&genesis, 0, u64::MAX)).is_err());
    let mut wrong_network = genesis.clone();
    wrong_network.network = "wrong".into();
    assert!(ledger.submit(transfer(&wrong_network, 0, 10)).is_err());
    let mut malformed = transfer(&genesis, 0, 10);
    malformed.signature.resize(65, 0);
    assert!(ledger.submit(malformed).is_err());
    assert_eq!(ledger.status().unwrap().tip, before.tip);
    assert_eq!(ledger.status().unwrap().state_root, before.state_root);
    assert_eq!(ledger.status().unwrap().mempool_bytes, 0);
}

#[test]
fn invalid_blocks_never_become_visible_or_persisted() {
    let temp = TempDir::new().unwrap();
    let genesis = config();
    let mut ledger = open(&temp, "node.sqlite", &genesis);
    let original = ledger.status().unwrap();
    let valid = seal(ledger.template([1; 32], 1000).unwrap());
    let mut variants = vec![];
    let mut bad = valid.clone();
    bad.header.state_root[0] ^= 1;
    variants.push(seal(bad));
    let mut bad = valid.clone();
    bad.header.transactions_root[0] ^= 1;
    variants.push(seal(bad));
    let mut bad = valid.clone();
    bad.header.target = genesis.maximum_target;
    variants.push(seal(bad));
    let mut bad = valid.clone();
    bad.header.height = 2;
    variants.push(seal(bad));
    let mut bad = valid.clone();
    bad.header.version = 2;
    variants.push(seal(bad));
    let mut bad = valid.clone();
    bad.header.timestamp_ms = 0;
    variants.push(seal(bad));
    let mut bad = valid.clone();
    bad.header.timestamp_ms = 200_000;
    variants.push(seal(bad));
    let mut bad = valid.clone();
    while valid_pow(&bad.header) {
        bad.header.nonce += 1;
    }
    variants.push(bad);
    for bad in variants {
        assert!(ledger.import(bad.clone(), 1000).is_err());
        assert!(!ledger.contains(bad.id()).unwrap());
        assert_eq!(ledger.status().unwrap().tip, original.tip);
    }
    assert!(ledger.import(valid.clone(), 1000).unwrap());
    let mut altered_body = valid;
    altered_body.transactions.push(transfer(&genesis, 0, 10));
    assert!(ledger.import(altered_body, 1000).is_err());
}

#[test]
fn duplicate_spend_inside_one_block_is_rejected_atomically() {
    let temp = TempDir::new().unwrap();
    let genesis = config();
    let mut ledger = open(&temp, "node.sqlite", &genesis);
    let transaction = transfer(&genesis, 0, 10);
    ledger.submit(transaction.clone()).unwrap();
    let before = ledger.status().unwrap();
    let mut block = ledger.template([1; 32], 1000).unwrap();
    block.transactions.push(transaction);
    block.header.transactions_root = Block::transactions_root(&block.transactions);
    let block = seal(block);
    assert!(ledger.import(block.clone(), 1000).is_err());
    assert!(!ledger.contains(block.id()).unwrap());
    assert_eq!(ledger.status().unwrap().tip, before.tip);
    assert_eq!(ledger.account([9; 32]).unwrap().balance, 0);
    assert_eq!(ledger.status().unwrap().mempool_count, 1);
}

#[test]
fn reward_cannot_be_spent_before_maturity() {
    let genesis = config();
    let key = SigningKey::from_bytes(&[8; 32]);
    let miner = address(&key.verifying_key().to_bytes());
    let initial = State::genesis(&genesis);
    let first = execute(&genesis, &initial, 1, miner, &[]).unwrap();
    assert_eq!(first.accounts[&miner].balance, 0);
    let spend = SignedTransfer::sign(
        Transfer {
            network: genesis.id(),
            public_key: key.verifying_key().to_bytes(),
            recipient: [9; 32],
            amount: 10,
            fee: 1,
            nonce: 0,
            valid_until: 10,
        },
        &key,
    )
    .unwrap();
    assert!(execute(&genesis, &first, 2, miner, std::slice::from_ref(&spend)).is_err());
    let second = execute(&genesis, &first, 2, miner, &[]).unwrap();
    let third = execute(&genesis, &second, 3, miner, &[spend]).unwrap();
    assert_eq!(third.accounts[&miner].balance, genesis.block_reward - 11);
}

#[test]
fn reorganization_rolls_back_state_and_requeues_displaced_transfers() {
    let temp = TempDir::new().unwrap();
    let genesis = config();
    let mut a = open(&temp, "a.sqlite", &genesis);
    let mut b = open(&temp, "b.sqlite", &genesis);
    a.submit(transfer(&genesis, 0, 25)).unwrap();
    mine(&mut a, [1; 32], 1000);
    let b1 = mine(&mut b, [2; 32], 1000);
    let b2 = mine(&mut b, [2; 32], 2000);
    assert!(!a.import(b1, 2000).unwrap()); // Equal work retains first tip.
    assert!(a.import(b2, 2000).unwrap());
    assert_eq!(
        a.status().unwrap().state_root,
        b.status().unwrap().state_root
    );
    assert_eq!(a.account([9; 32]).unwrap().balance, 0);
    assert_eq!(a.status().unwrap().mempool_count, 1);
    let final_block = mine(&mut a, [1; 32], 3000);
    b.import(final_block, 3000).unwrap();
    assert_eq!(a.account([9; 32]).unwrap().balance, 25);
    assert_eq!(
        a.status().unwrap().state_root,
        b.status().unwrap().state_root
    );
}

#[test]
fn cumulative_work_can_choose_a_shorter_chain() {
    let temp = TempDir::new().unwrap();
    let genesis = config();
    let mut slow = open(&temp, "slow.sqlite", &genesis);
    let mut fast = open(&temp, "fast.sqlite", &genesis);
    for height in 1..=8 {
        mine(&mut slow, [1; 32], height * 1000);
    }
    for height in 1..=6 {
        let block = mine(&mut fast, [2; 32], height);
        slow.import(block, 10_000).unwrap();
    }
    assert_eq!(slow.status().unwrap().height, 6);
    assert_eq!(slow.status().unwrap().tip, fast.status().unwrap().tip);
}

#[test]
fn multi_page_sync_crosses_a_fork_longer_than_one_page() {
    let temp = TempDir::new().unwrap();
    let genesis = config();
    let mut a = open(&temp, "a.sqlite", &genesis);
    let mut b = open(&temp, "b.sqlite", &genesis);
    for height in 1..=35 {
        mine(&mut a, [1; 32], height * 1000);
    }
    for height in 1..=40 {
        mine(&mut b, [2; 32], height * 1000);
    }
    let mut cursor = None;
    for _ in 0..4 {
        let response = b
            .sync_page(SyncRequest {
                network: genesis.id(),
                locator: a.locator(cursor).unwrap(),
            })
            .unwrap();
        assert!(response.blocks.len() <= SYNC_PAGE);
        for block in response.blocks {
            cursor = Some(block.id());
            a.import(block, 50_000).unwrap();
        }
    }
    assert_eq!(a.status().unwrap().tip, b.status().unwrap().tip);
    assert_eq!(a.canonical_ids().unwrap(), b.canonical_ids().unwrap());
    assert!(b
        .sync_page(SyncRequest {
            network: [0; 32],
            locator: a.locator(None).unwrap()
        })
        .is_err());
    assert!(b
        .sync_page(SyncRequest {
            network: genesis.id(),
            locator: vec![]
        })
        .is_err());
}

#[test]
fn retarget_boundary_is_integer_and_clamped() {
    let genesis = config();
    assert_eq!(
        next_target(&genesis, &[], 1).unwrap(),
        genesis.initial_target
    );
    let headers: Vec<Header> = (1..=4)
        .map(|height| Header {
            version: VERSION,
            network: genesis.id(),
            parent: [0; 32],
            height,
            timestamp_ms: height,
            target: genesis.initial_target,
            miner: [0; 32],
            transactions_root: [0; 32],
            state_root: [0; 32],
            nonce: 0,
        })
        .collect();
    assert_eq!(
        next_target(&genesis, &headers, 5).unwrap(),
        genesis.initial_target / 4
    );
    let mut slow = headers;
    slow.last_mut().unwrap().timestamp_ms = 100_000;
    assert_eq!(
        next_target(&genesis, &slow, 5).unwrap(),
        genesis.maximum_target
    );
}

#[test]
fn mempool_count_is_bounded_and_rejects_expired_transactions() {
    let temp = TempDir::new().unwrap();
    let mut genesis = config();
    let keys: Vec<_> = (0..=MAX_POOL_COUNT)
        .map(|n| {
            let mut seed = [0; 32];
            seed[..8].copy_from_slice(&(n as u64).to_le_bytes());
            SigningKey::from_bytes(&seed)
        })
        .collect();
    genesis.allocations = keys
        .iter()
        .map(|key| (address(&key.verifying_key().to_bytes()), 100))
        .collect();
    genesis.allocations.sort();
    let mut ledger = open(&temp, "node.sqlite", &genesis);
    for (index, key) in keys.iter().enumerate() {
        let tx = SignedTransfer::sign(
            Transfer {
                network: genesis.id(),
                public_key: key.verifying_key().to_bytes(),
                recipient: [9; 32],
                amount: 1,
                fee: 1,
                nonce: 0,
                valid_until: 1,
            },
            key,
        )
        .unwrap();
        assert_eq!(ledger.submit(tx).is_ok(), index < MAX_POOL_COUNT);
    }
    assert_eq!(ledger.status().unwrap().mempool_count, MAX_POOL_COUNT);
    mine(&mut ledger, [1; 32], 1000);
    assert_eq!(ledger.status().unwrap().mempool_count, 0); // Remaining entries expired.
}
