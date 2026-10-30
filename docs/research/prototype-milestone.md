# First independent prototype — verification record

Date: 2026-10-30 (Asia/Seoul). Host: macOS 15.5, arm64; Homebrew Rust/Cargo 1.93.1. These results concern Nexium's original development prototype, not upstream Starcoin.

## Delivered

Five Rust workspace crates: protocol primitives, development consensus/execution, persistent ledger, local node/CLI and isolated account-based Move adapter. Exact direct dependency versions and the lockfile are included. No Starcoin node, genesis, framework, block permit or administrative authority was imported. See [dependency baseline](../dependency-baseline.md) for the exact Move revision and its qualification limits.

The local native-transfer chain supports signature verification, replay/expiry/nonce checks, reward maturity, development CPU PoW, difficulty adjustment, cumulative-work selection, validated side branches, atomic canonical reorganization, bounded mempool admission, SQLite recovery, local peer exchange and paged synchronization. The adapter verifies module ownership/bytecode and tests compatible compiler/VM execution; the node does not yet accept Move calls.

## Checks completed

| Check | Result |
|---|---|
| `cargo test --workspace --locked --offline` using populated isolated cache | 14 tests passed: 10 ledger/consensus integration tests and 4 Move adapter tests; documentation tests also completed |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | Passed |
| `cargo build -p nexium-node --locked --offline` | Passed |
| `python3 scripts/devnet_smoke.py` against final rebuilt binary | Passed: three local node processes, signed transfer gossip/inclusion, killed follower, restart/catch-up, common canonical tip and state root, wrong-network rejection |
| Python source syntax and local Markdown link/fence checks | Passed |

The environment's default filesystem/network sandbox prevents loopback socket binding. The smoke test was run with approved loopback access. It used temporary databases, a public development funding key and a temporary random recipient key; the harness stopped its processes and removed its data.

During development, an initial offline test attempt required fetching a missing test dependency. One intermediate documentation-test run failed to resolve built dependencies while Cargo build/test jobs had overlapped; the exact cause was not established. The final complete test suite was rerun without overlapping Cargo builds and passed. An initial Clippy finding was corrected before the final pass.

## Final smoke evidence

The smoke harness used a 200 ms development mining interval for quick functional coverage, not the normal ten-second default. At stable convergence after mining stopped:

```json
{
  "result": "passed",
  "nodes": 3,
  "height": 9,
  "tip": "00884b7178ea6c23e017bf7fbf068a27dafbc8f7ac6db471f2ea029c5fac5543",
  "state_root": "4fac95086bc995b8732840897819710e469fa074bcc5113826ec8a039ea6c82f",
  "transfer": "58d21d594cb71c4dc9781bb786d1c496f181b0199583514bc86c9d378ba687d3",
  "crash_restart": true,
  "hardware_benchmark": false
}
```

Hashes differ across executions because wall-clock timestamps and recipient keys vary. Reproducibility means the same invariants and checks pass, not that independent smoke runs produce identical chains.

Unit/integration coverage also verifies shorter-but-higher-work branch selection, equal-work first-tip retention, side-branch synchronization longer than one page, displaced-transfer reconsideration, duplicate spends within a block, malformed/wrong-network/replayed transfers, tampered commitments/difficulty/headers, invalid PoW, retarget clamping, immature rewards, database ownership, genesis mismatch, durable restart and bounded/expired mempool entries.

## Scope and next milestones

This establishes a working local development chain, not completion of Phase 1 for an adversarial public network. The [implemented protocol](../development-protocol.md) specifies temporary limits and trust assumptions.

1. Design metered Move transactions, sender authorization, resource persistence, gas rules and minimal framework. Verify rollback/module-cache behavior before accepting untrusted bytecode.
2. Qualify CPU-friendly mining, difficulty/timestamp rules, issuance assumptions and adversarial fork behavior; replace development PoW before public deployment.
3. Extend networking to production P2P/discovery and independently hosted nodes; qualify Linux. The included CI workflow targets Linux/macOS, but hosted CI was not run in this session.
4. Replace full-state snapshots/hash commitments with scalable authenticated state/retention and profile database choices. Compare sequential/parallel execution only after the reference Move path is correct.
5. Perform the [benchmark plan](../benchmark-plan.md) on physical 16 GB machines. No sustained TPS, RAM ceiling, independent-hardware operation, privacy or production security result is claimed by this milestone.
