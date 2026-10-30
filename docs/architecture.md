# Nexium architecture proposal

Date: 2026-10-30. Accepted foundation: an independent Nexium codebase with selective component reuse. Detailed architecture and dependencies remain research proposals. The user has authorized a development prototype. See [implemented scope](development-protocol.md) and [dependency baseline](dependency-baseline.md). See [technical decisions](technical-decisions.md).

## Baseline

Use an independently validating conventional PoW chain with approximately ten-second target spacing. The user has selected account/resource-based Move as the state and programming model. Keep one supported Move VM dialect and deterministic sequential execution as the reference, then introduce deterministic optimistic parallel execution. The initial VM/compiler/verifier revision is pinned in the dependency baseline. Framework, gas schedule and parallel executor remain unselected. Native public transfers precede consensus-integrated Move execution.

Mining is a local service with bounded worker count and lower scheduling priority. Block validation, P2P processing and durable commits must retain resources during mining and overload. Local worker count must never alter consensus validity or gas charges.

## Proposed boundaries

Long-term workspace boundaries (the initial workspace combines some boundaries in the ledger and node crates):

```text
crates/
  primitives/    # Canonical encoding, transaction/header types, hashes
  consensus/     # PoW validation, chainwork, difficulty, timestamps
  chain/         # Validated block graph, canonical tip, reorganizations
  state/         # Versioned authenticated state and proof interfaces
  storage/       # Durable batches, recovery, retention
  move-adapter/  # Pinned VM, verifier, natives, gas and framework boundary
  executor/      # Sequential reference and parallel implementation
  mempool/       # Bounded admission, expiry, selection and reorg reinsertion
  network/       # Discovery, propagation, sync, peer/resource limits
  mining/        # Templates, PoW workers and cancellation
  rpc/           # Bounded public/local API
  node/          # Configuration and lifecycle
benchmarks/
tests/
docs/
```

Keep reused components traceable to their source revisions and preserve licenses, attribution and relevant tests. Avoid importing Starcoin’s historical consensus exceptions, block permits, genesis authorities or dual-VM machinery. Add privacy only when that phase begins; define transaction versioning and commitment boundaries early enough to allow it.

## Consensus contract to specify before implementation

- Canonical header binds parent, transaction commitment, post-state commitment, timestamp, target/difficulty, nonce, version and network-specific domain.
- Every node checks encoding, size/resource limits, parent validity, timestamps, expected difficulty, PoW, signatures, transaction order, gas, rewards and resulting commitments.
- Select the valid chain with greatest cumulative work using an explicitly specified target-to-work formula and overflow-safe arithmetic. Height alone is insufficient. Define equal-work local preference and convergence behavior.
- Difficulty must be computed only from validated ancestor data with integer arithmetic and fixed boundary rules. Choose and document a proven adjustment design after reviewing timestamp manipulation, hash-rate shocks and oscillation. Ten seconds is a target, not a deadline.
- Specify median-time and future-time handling, including how temporarily future blocks are reconsidered. Distinguish consensus timestamp rules from local clock admission policy.
- Reorganizations restore authenticated state at the common ancestor, apply the winning branch, atomically publish canonical metadata, and reconsider displaced transactions. Define recovery when pruning has removed needed data; never silently reject a valid heavier chain because of a local retention policy.
- Specify issuance sufficient to make reward validation testable, coinbase maturity, replay protection, transaction expiry and gas rules. Economic tuning remains separate from correctness.
- Confirmations are probabilistic. RPC and benchmarks must distinguish inclusion from a chosen confirmation depth.

CryptoNight-R and RandomX are candidates for comparison; neither has been selected. RandomX must be assessed for verification costs, initialization, native safety and mining/validation contention before selection.

The [RandomX reference documentation](https://github.com/tevador/RandomX#overview) specifies 2,080 MiB shared memory for fast mode and 256 MiB for slower light verification; its hardware guidance calls for at least 2.5 GiB free per NUMA node. Include cache/dataset transitions, per-worker allocations and key initialization in the budget. Its C API, arm64 JIT and BSD-3-Clause source make it a concrete investigation candidate, but no Rust binding or integration has been selected. Do not copy its example key-change block counts unchanged into a ten-second-block chain: define key timing and reorg behavior for this network.

## Execution and storage

The Move adapter owns the exact bytecode version, verifier limits, native function implementations, gas schedule, transaction prologue/epilogue, publication and upgrade rules. VM alone does not provide blockchain transaction semantics. Pin the full toolchain/framework combination and test macOS arm64 against Linux x86_64.

Parallel execution must reproduce sequential status, events, gas and state effects for the same ordered block. Bound speculative memory and reexecution work, and retain an explicit sequential fallback. Compare independent transfers, hot accounts, shared resources, module publication and failing transactions.

State exposes read snapshots and ordered write batches; execution never directly publishes the canonical tip. Persist state nodes, transaction results and canonical metadata with a documented crash-consistent protocol. Budget all caches, write buffers, block bodies, VM caches, queues and native allocations. Historical pruning must protect retained forks and in-flight readers.

## Networking and trust

Use peer discovery, bounded propagation and streaming synchronization with per-peer request, byte and CPU budgets. Configure several independently operated discovery paths. A bootnode provides connectivity, not transaction authority. Validate fetched data locally and bound orphan/invalid-block caches.

Offer genesis replay as the strongest validation mode. If snapshots are later added, document exactly how state roots are authenticated and which execution history was skipped. Pruned peers cannot guarantee genesis data availability; plan archival sourcing without making one provider authoritative.

## Selected state model and alternative considered

| Model | Fit | Required experiment |
|---|---|---|
| Account/resources + optimistic parallelism | Strong Starcoin reuse and general shared-state contracts | Conflict/retry behavior, hot sequence numbers and fee accounting |
| Objects + declared dependencies | Explicit ownership and potentially simpler scheduling for independent objects | VM/framework adaptation, shared-object ordering and programming semantics |

Account/resource-based Move is the selected approach. The object model remains a documented alternative, not a parallel implementation workstream. Objects do not remove contention over shared resources, and an object model is not supplied merely by selecting Move. Validate the selected model using representative independent and conflicting workloads; no memory or throughput advantage is assumed without measurements.
