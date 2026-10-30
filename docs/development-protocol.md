# Development protocol v1

This documents the implemented native-transfer prototype, not a mainnet specification. User authorization to start implementation supersedes the earlier no-code pause. The long-term design remains in [architecture](architecture.md); this document states what exists now.

## Identity and encoding

The canonical genesis manifest includes protocol version, network name, timestamp in milliseconds, initial/maximum PoW targets, spacing, retarget interval, block reward, reward maturity and sorted unique allocations. The SHA3-256 domain-separated BCS manifest digest identifies the network. Allocation order is validated. Genesis creates a synthetic height-zero block with zero chainwork and a committed initial state; it is not mined.

Every hash prefixes its domain with an eight-byte little-endian domain-length field, then hashes the domain bytes and canonical BCS payload. Protocol types are fixed field sequences. Header IDs commit version, network, parent, height, timestamp, target, miner, transaction commitment, state commitment and nonce. The transaction commitment is a hash of the entire ordered signed-transfer list; it is not yet a Merkle tree. Consensus encoding does not depend on JSON field order or database row order.

Addresses are SHA3-derived, domain-separated 32-byte Ed25519 public-key identities. This is a Nexium development format; it is not a promise of Aptos, Starcoin or Sui address compatibility. The isolated Move VM is configured for 32-byte addresses.

## Transactions and execution

A signed native transfer carries genesis/network digest, public key, recipient, amount, fee, sequence nonce and expiry height. Ed25519 strict verification signs the domain-separated transfer digest; the signed envelope has its own ID. Signature length is exactly 64 bytes. Positive amount, correct network, exact sender nonce, sufficient funds including fee, nonexpired height and checked arithmetic are mandatory.

Transfers execute in the explicit block order against a temporary parent state. Self-transfers increment the nonce and deduct only the fee. Failed native transfers invalidate the entire block; there are no partially committed or failed-but-charged native transactions in this prototype. Fees and the fixed block subsidy become an immature miner reward. Rewards unlock at creation height plus genesis maturity, before transactions of that height execute. New miner identities reserve an account slot when the reward is created. No signature from a foundation, authority or genesis account is required to mine or validate blocks.

State is a sorted account map plus ordered immature rewards. Each account has u64 balance and nonce. Checked balance, cost, reward, nonce and height operations reject overflow. Full-state BCS hashing provides a deterministic commitment, but not an authenticated query proof. Nodes independently execute blocks; sync never imports a peer's computed state as authoritative.

## PoW and chain selection

Development PoW compares the first eight bytes of the SHA3-256 header digest, interpreted as an unsigned big-endian integer, against a u64 target. The full 256-bit header digest remains the block ID. Work is `floor(2^64 / (target + 1))`, accumulated with checked u128 arithmetic. Targets are nonzero and the maximum is below u64::MAX. This 64-bit threshold construction and cheap SHA3 mining are deliberately unsuitable as a production security commitment. CryptoNight-R/RandomX selection remains open.

The first N blocks use the initial target, where N is the retarget interval. At heights `N+1, 2N+1, ...`, use the N parent-branch headers ending at the parent. Expected elapsed time is `(N-1) * spacing_ms`; actual elapsed time is the difference between last and first timestamps, clamped to one-quarter through four times the expected duration. New target is `floor(parent_target * clamped_elapsed / expected_elapsed)`, clamped to `[1, maximum_target]`. All operations use integer arithmetic. This simple development retarget is not yet adversarially qualified for deployment.

Each block timestamp must strictly exceed its parent's. Local admission postpones blocks more than 120 seconds ahead of the local clock; peers can retry them later. There is no invalid-block blacklist preventing reconsideration. Mining defaults to ten-second polling intervals from the manifest, but actual blocks are not guaranteed ten seconds apart. One miner worker yields between bounded hash batches and cancels when its parent ceases to be the canonical tip. OS-level mining priority and a resource-isolated production miner are future work.

Every side-branch block must pass the same validation as a canonical block. Choose greatest cumulative work, not height; retain the first accepted canonical tip on equal work. Find a common canonical ancestor, atomically replace canonical suffix metadata and persist the new tip. State snapshots on each validated block provide rollback. Reconsider displaced transfers under the new canonical state, bounded by normal admission rules. Reorgs have no privileged depth limit. Confirmations remain probabilistic.

## Persistence and limits

SQLite uses WAL mode, synchronous FULL, an 8 MiB page-cache target and disabled mmap. A filesystem lock permits one node process per database. Block bytes, resulting state snapshot, chainwork, canonical mappings and tip changes are committed in a single database transaction before updating the in-memory tip. On startup check genesis equality, canonical tip consistency and tip state commitment. Local database contents are trusted storage; this startup check is not a full corruption audit or genesis replay.

Prototype limits:

- 256 native transactions per block and 512 KiB canonical block bytes.
- 100,000 accounts; rewards reserve miner accounts before maturity.
- 1,024 pending transactions, 1 MiB canonical transaction bytes and one pending transaction per sender.
- 100,000 stored blocks across all branches. Reaching this development cap stops import; no pruning exists.
- 16 configured peer tasks, 16 concurrent HTTP requests, 2 MiB inbound body limit.
- At most 16 blocks per sync page, 64 locator IDs, 16 MiB downloaded sync response and 2 MiB downloaded pending-transaction response.
- Four-second outbound request timeout and ten-second inbound handler timeout.

The pool is volatile. Restart discards pending entries; submitters must retry based on nonce/chain state. Admission is bounded best-effort acceptance, not guaranteed inclusion. Native fees have no market-based eviction yet. Side-branch history and full state snapshots accumulate until the development storage cap. Those mechanisms establish finite prototype limits, not economical sustained storage or the 16 GB production requirement. Full-state cloning/hashing and per-block snapshots must be replaced before meaningful throughput qualification.

## Network and remaining work

Configured numeric loopback peers poll for validated canonical block pages and pending transactions. Each peer cursor can reference a validated side branch, allowing forks longer than one page to synchronize before becoming canonical. Peer endpoints cannot redirect and do not inherit environment HTTP proxies. There is no discovery, libp2p, encrypted transport, peer identity/reputation or production DoS defense yet. HTTP and JSON are integration scaffolding.

Move is deliberately a separate adapter. Its VM/verifier/compiler share one exact source revision; no Starcoin framework, native-function registry, genesis code or block permits are inherited. A trusted test fixture can compile and execute a constant-return module. The node dispatches native transfers only. Authenticated publication, metered contract execution, resource persistence, framework, gas schedule, upgrades and reorg-aware module handling are required before Move transactions are accepted by consensus. Unmetered fixture execution is confined to tests.
