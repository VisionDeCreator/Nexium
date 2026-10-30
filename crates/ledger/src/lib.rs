//! Atomic SQLite development ledger. Full state per block is intentionally temporary.
use anyhow::{bail, ensure, Context, Result};
use nexium_consensus::{apply_transfer, execute, mature, next_target, validate, work};
use nexium_primitives::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    path::Path,
    time::Duration,
};

pub const MAX_POOL_COUNT: usize = 1024;
pub const MAX_POOL_BYTES: usize = 1024 * 1024;
pub const SYNC_PAGE: usize = 16;
pub const MAX_LOCATORS: usize = 64;
pub const MAX_STORED_BLOCKS: u64 = 100_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Status {
    pub network: Hash,
    pub tip: Hash,
    pub height: u64,
    pub state_root: Hash,
    pub chainwork: String,
    pub mempool_count: usize,
    pub mempool_bytes: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncRequest {
    pub network: Hash,
    pub locator: Vec<Hash>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncResponse {
    pub network: Hash,
    pub blocks: Vec<Block>,
}

struct Stored {
    block: Block,
    state: State,
    chainwork: u128,
}

pub struct Ledger {
    _lock: File,
    db: Connection,
    genesis: Genesis,
    network: Hash,
    tip: Hash,
    // One transaction per sender. FIFO admission order; block ordering is explicit.
    pool: Vec<SignedTransfer>,
    pool_bytes: usize,
    pool_ids: BTreeSet<Hash>,
    pool_senders: BTreeSet<Address>,
}

impl Ledger {
    pub fn open(path: &Path, genesis: Genesis) -> Result<Self> {
        genesis.validate()?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("lock"))?;
        lock.try_lock().context("database is already in use")?;
        let db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(2))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            PRAGMA cache_size=-8192; PRAGMA mmap_size=0;
            CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS blocks (
                id BLOB PRIMARY KEY, parent BLOB NOT NULL, height INTEGER NOT NULL,
                body BLOB NOT NULL, state BLOB NOT NULL, chainwork BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS canonical (height INTEGER PRIMARY KEY, id BLOB NOT NULL);",
        )?;
        let saved: Option<Vec<u8>> = db
            .query_row("SELECT value FROM meta WHERE key='genesis'", [], |r| {
                r.get(0)
            })
            .optional()?;
        let tip;
        if let Some(saved) = saved {
            ensure!(
                saved == bcs::to_bytes(&genesis)?,
                "database genesis mismatch"
            );
            let bytes: Vec<u8> =
                db.query_row("SELECT value FROM meta WHERE key='tip'", [], |r| r.get(0))?;
            tip = bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("corrupt tip"))?;
        } else {
            let state = State::genesis(&genesis);
            let block = Block {
                header: Header {
                    version: VERSION,
                    network: genesis.id(),
                    parent: [0; 32],
                    height: 0,
                    timestamp_ms: genesis.timestamp_ms,
                    target: genesis.initial_target,
                    miner: [0; 32],
                    transactions_root: Block::transactions_root(&[]),
                    state_root: state.root(),
                    nonce: 0,
                },
                transactions: vec![],
            };
            tip = block.id();
            let tx = db.unchecked_transaction()?;
            tx.execute(
                "INSERT INTO meta VALUES ('genesis', ?1)",
                [bcs::to_bytes(&genesis)?],
            )?;
            tx.execute("INSERT INTO meta VALUES ('tip', ?1)", [tip.as_slice()])?;
            tx.execute(
                "INSERT INTO blocks VALUES (?1, ?2, 0, ?3, ?4, ?5)",
                params![
                    tip.as_slice(),
                    [0u8; 32].as_slice(),
                    bcs::to_bytes(&block)?,
                    bcs::to_bytes(&state)?,
                    0u128.to_le_bytes().as_slice()
                ],
            )?;
            tx.execute("INSERT INTO canonical VALUES (0, ?1)", [tip.as_slice()])?;
            tx.commit()?;
        }
        let network = genesis.id();
        let ledger = Self {
            _lock: lock,
            db,
            genesis,
            network,
            tip,
            pool: vec![],
            pool_bytes: 0,
            pool_ids: BTreeSet::new(),
            pool_senders: BTreeSet::new(),
        };
        // Detect inconsistent tip/state metadata on startup. SQLite handles torn transactions.
        let stored = ledger.stored(tip)?;
        ensure!(
            stored.block.header.state_root == stored.state.root(),
            "stored state commitment mismatch"
        );
        let canonical: Vec<u8> = ledger.db.query_row(
            "SELECT id FROM canonical ORDER BY height DESC LIMIT 1",
            [],
            |r| r.get(0),
        )?;
        ensure!(canonical == tip, "canonical tip mismatch");
        Ok(ledger)
    }

    fn stored(&self, id: Hash) -> Result<Stored> {
        let (body, state, chainwork): (Vec<u8>, Vec<u8>, Vec<u8>) = self.db.query_row(
            "SELECT body, state, chainwork FROM blocks WHERE id=?1",
            [id.as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        Ok(Stored {
            block: bcs::from_bytes(&body)?,
            state: bcs::from_bytes(&state)?,
            chainwork: u128::from_le_bytes(
                chainwork
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("corrupt chainwork"))?,
            ),
        })
    }

    pub fn contains(&self, id: Hash) -> Result<bool> {
        Ok(self
            .db
            .query_row("SELECT 1 FROM blocks WHERE id=?1", [id.as_slice()], |_| {
                Ok(())
            })
            .optional()?
            .is_some())
    }

    pub fn status(&self) -> Result<Status> {
        let tip = self.stored(self.tip)?;
        Ok(Status {
            network: self.genesis.id(),
            tip: self.tip,
            height: tip.block.header.height,
            state_root: tip.block.header.state_root,
            chainwork: tip.chainwork.to_string(),
            mempool_count: self.pool.len(),
            mempool_bytes: self.pool_bytes,
        })
    }

    pub fn network_id(&self) -> Hash {
        self.network
    }

    pub fn account(&self, address: Address) -> Result<Account> {
        Ok(self
            .stored(self.tip)?
            .state
            .accounts
            .get(&address)
            .cloned()
            .unwrap_or_default())
    }

    pub fn pending(&self) -> Vec<SignedTransfer> {
        self.pool.clone()
    }

    pub fn submit(&mut self, transaction: SignedTransfer) -> Result<Hash> {
        ensure!(
            transaction.signature.len() == 64,
            "invalid signature length"
        );
        let id = transaction.id();
        if self.pool_ids.contains(&id) {
            return Ok(id);
        }
        ensure!(self.pool.len() < MAX_POOL_COUNT, "mempool full");
        let sender = transaction.sender();
        ensure!(
            !self.pool_senders.contains(&sender),
            "sender already has a pending transaction"
        );
        let size = bcs::to_bytes(&transaction)?.len();
        ensure!(
            self.pool_bytes
                .checked_add(size)
                .is_some_and(|n| n <= MAX_POOL_BYTES),
            "mempool byte limit"
        );
        let tip = self.stored(self.tip)?;
        let next_height = tip
            .block
            .header
            .height
            .checked_add(1)
            .context("height overflow")?;
        let mut state = tip.state;
        mature(&mut state, next_height)?;
        // Validate against canonical state, without assuming other pending transactions execute.
        apply_transfer(&mut state, &transaction, self.network, next_height)?;
        self.pool_bytes += size;
        self.pool_ids.insert(id);
        self.pool_senders.insert(sender);
        self.pool.push(transaction);
        Ok(id)
    }

    fn ancestors(&self, mut id: Hash, limit: usize) -> Result<Vec<Header>> {
        let mut result = vec![];
        for _ in 0..limit {
            let header = self.stored(id)?.block.header;
            let height = header.height;
            id = header.parent;
            result.push(header);
            if height == 0 {
                break;
            }
        }
        result.reverse();
        Ok(result)
    }

    pub fn template(&self, miner: Address, now_ms: u64) -> Result<Block> {
        let parent = self.stored(self.tip)?;
        let height = parent
            .block
            .header
            .height
            .checked_add(1)
            .context("height overflow")?;
        let ancestors = self.ancestors(self.tip, self.genesis.retarget_interval as usize)?;
        let mut accepted = vec![];
        let mut provisional = parent.state.clone();
        mature(&mut provisional, height)?;
        for transaction in self.pool.iter().take(MAX_TRANSACTIONS) {
            let mut next = provisional.clone();
            if apply_transfer(&mut next, transaction, self.genesis.id(), height).is_ok() {
                provisional = next;
                accepted.push(transaction.clone());
            }
        }
        let state = execute(&self.genesis, &parent.state, height, miner, &accepted)?;
        Ok(Block {
            header: Header {
                version: VERSION,
                network: self.genesis.id(),
                parent: self.tip,
                height,
                timestamp_ms: now_ms.max(
                    parent
                        .block
                        .header
                        .timestamp_ms
                        .checked_add(1)
                        .context("timestamp overflow")?,
                ),
                target: next_target(&self.genesis, &ancestors, height)?,
                miner,
                transactions_root: Block::transactions_root(&accepted),
                state_root: state.root(),
                nonce: 0,
            },
            transactions: accepted,
        })
    }

    /// Validate every block, including side branches, before storing it. Return tip change.
    pub fn import(&mut self, block: Block, now_ms: u64) -> Result<bool> {
        ensure!(
            block.transactions.len() <= MAX_TRANSACTIONS,
            "too many transactions"
        );
        ensure!(
            block.transactions.iter().all(|tx| tx.signature.len() == 64),
            "invalid signature length"
        );
        ensure!(
            bcs::to_bytes(&block)?.len() <= MAX_BLOCK_BYTES,
            "block too large"
        );
        let id = block.id();
        if self.contains(id)? {
            // A known header must not allow a different body to bypass commitment checks.
            ensure!(
                self.stored(id)?.block == block,
                "known header with different body"
            );
            return Ok(false);
        }
        let count: u64 = self
            .db
            .query_row("SELECT count(*) FROM blocks", [], |r| r.get(0))?;
        ensure!(
            count < MAX_STORED_BLOCKS,
            "development block storage limit reached"
        );
        let parent = self
            .stored(block.header.parent)
            .context("unknown block parent")?;
        let ancestors =
            self.ancestors(block.header.parent, self.genesis.retarget_interval as usize)?;
        let state = validate(
            &self.genesis,
            &parent.block.header,
            &parent.state,
            &ancestors,
            &block,
            now_ms,
        )?;
        let chainwork = parent
            .chainwork
            .checked_add(work(block.header.target))
            .context("chainwork overflow")?;
        let old = self.stored(self.tip)?;
        // Equal-work branches retain the first accepted tip; eventually greater work wins.
        let wins = chainwork > old.chainwork;
        let mut branch = vec![];
        let mut displaced = vec![];
        let mut fork_height = 0;
        if wins {
            let mut current = block.clone();
            loop {
                let canonical: Option<Vec<u8>> = self
                    .db
                    .query_row(
                        "SELECT id FROM canonical WHERE height=?1",
                        [current.header.height],
                        |r| r.get(0),
                    )
                    .optional()?;
                if canonical.as_deref() == Some(current.id().as_slice()) {
                    fork_height = current.header.height;
                    break;
                }
                ensure!(current.header.height > 0, "no common genesis");
                let parent_id = current.header.parent;
                branch.push((current.header.height, current.id()));
                current = self.stored(parent_id)?.block;
            }
            let mut statement = self.db.prepare("SELECT b.body FROM canonical c JOIN blocks b ON c.id=b.id WHERE c.height>?1 ORDER BY c.height")?;
            let bodies = statement.query_map([fork_height], |r| r.get::<_, Vec<u8>>(0))?;
            for body in bodies {
                displaced.extend(bcs::from_bytes::<Block>(&body?)?.transactions);
            }
        }
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT INTO blocks VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id.as_slice(),
                block.header.parent.as_slice(),
                block.header.height,
                bcs::to_bytes(&block)?,
                bcs::to_bytes(&state)?,
                chainwork.to_le_bytes().as_slice()
            ],
        )?;
        if wins {
            tx.execute("DELETE FROM canonical WHERE height>?1", [fork_height])?;
            for (height, block_id) in &branch {
                tx.execute(
                    "INSERT INTO canonical VALUES (?1, ?2)",
                    params![height, block_id.as_slice()],
                )?;
            }
            tx.execute("UPDATE meta SET value=?1 WHERE key='tip'", [id.as_slice()])?;
        }
        tx.commit()?;
        if wins {
            self.tip = id;
            let mut pending = std::mem::take(&mut self.pool);
            pending.extend(displaced);
            self.pool_bytes = 0;
            self.pool_ids.clear();
            self.pool_senders.clear();
            for transfer in pending {
                let _ = self.submit(transfer);
            }
        }
        Ok(wins)
    }

    /// Cursor can be on a validated side branch during a multi-page reorganization sync.
    pub fn locator(&self, cursor: Option<Hash>) -> Result<Vec<Hash>> {
        let mut locator = vec![];
        if let Some(cursor) = cursor {
            if self.contains(cursor)? {
                locator.push(cursor);
            }
        }
        let tip = self.status()?.height;
        let mut height = tip;
        let mut step = 1;
        loop {
            let id: Vec<u8> =
                self.db
                    .query_row("SELECT id FROM canonical WHERE height=?1", [height], |r| {
                        r.get(0)
                    })?;
            let id: Hash = id
                .try_into()
                .map_err(|_| anyhow::anyhow!("corrupt canonical id"))?;
            if !locator.contains(&id) {
                locator.push(id);
            }
            if height == 0 {
                break;
            }
            height = height.saturating_sub(step);
            if locator.len() > 10 {
                step = step.saturating_mul(2);
            }
        }
        ensure!(locator.len() <= MAX_LOCATORS, "locator limit exceeded");
        Ok(locator)
    }

    pub fn sync_page(&self, request: SyncRequest) -> Result<SyncResponse> {
        ensure!(request.network == self.genesis.id(), "wrong network");
        ensure!(
            !request.locator.is_empty() && request.locator.len() <= MAX_LOCATORS,
            "invalid locator size"
        );
        let mut found = None;
        for id in request.locator {
            let height = self
                .db
                .query_row(
                    "SELECT height FROM canonical WHERE id=?1",
                    [id.as_slice()],
                    |r| r.get::<_, u64>(0),
                )
                .optional()?;
            if height.is_some() {
                found = height;
                break;
            }
        }
        let Some(height) = found else {
            bail!("no common genesis");
        };
        let mut statement = self.db.prepare("SELECT b.body FROM canonical c JOIN blocks b ON c.id=b.id WHERE c.height>?1 ORDER BY c.height LIMIT ?2")?;
        let bodies = statement.query_map(params![height, SYNC_PAGE], |r| r.get::<_, Vec<u8>>(0))?;
        let mut blocks = vec![];
        for body in bodies {
            blocks.push(bcs::from_bytes(&body?)?);
        }
        Ok(SyncResponse {
            network: self.genesis.id(),
            blocks,
        })
    }

    pub fn canonical_ids(&self) -> Result<BTreeSet<Hash>> {
        let mut statement = self.db.prepare("SELECT id FROM canonical")?;
        let rows = statement.query_map([], |r| r.get::<_, Vec<u8>>(0))?;
        rows.map(|row| row?.try_into().map_err(|_| anyhow::anyhow!("corrupt id")))
            .collect()
    }
}
