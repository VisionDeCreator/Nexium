# Technical decisions and implementation priorities

Date: 2026-10-30. Status: user authorized development implementation. Phase 0 component qualification continues alongside the first prototype.

## Decisions

**Accepted direction:** The user selected an independent Nexium Rust codebase with selective reuse of established components. Do not fork Starcoin wholesale. Phase 0 now evaluates components and compatible groups of dependencies; the later user instruction to start authorizes the first development prototype. See [dependency baseline](dependency-baseline.md) and [implemented development protocol](development-protocol.md).

Follow-up correction: v1.13.22 inherits authority-signed block permits introduced in v1.13.21. It is not permissionless as shipped. Any evaluation fork must explicitly exclude that policy and review all affected paths; compare a pre-permit baseline and selected reuse. See the correction in [Starcoin assessment](starcoin-assessment.md). This finding increases the fork's adaptation scope.

| Topic | Position | Reason / reopen condition |
|---|---|---|
| Foundation | Independent Nexium codebase with selective component reuse; accepted by user | Explicit ownership of protocol rules and authorities; reuse mature implementations where compatible |
| Default Starcoin branch | Do not adopt for initial chain | DAG and dual VM expand consensus and maintenance scope |
| Consensus | Conventional cumulative-work PoW; ten-second target | Matches brief and provides a tractable baseline |
| VM | Single pinned account-based Move compiler/VM baseline; framework unselected | Minimize semantic ambiguity; parallel mode must match exactly |
| State | Account/resource-based Move; selected by user | Fits ordered PoW execution and component reuse; initial pinned lineage is documented in dependency-baseline.md |
| Execution | Sequential reference, then deterministic optimistic parallel execution | Parallel status, events, gas and state commitments must match the sequential result; bound speculative resources |
| Mining | Compare CryptoNight-R and RandomX implementations | CPU friendliness and resource interference require measurement |
| Privacy | Deferred shielded transfers | No arbitrary private contracts; select established constructions later |
| Upgrades | No inherited emergency authority | Trace capabilities, framework privileges and consensus authorization in every selected component |
| Optimization | Profile whole nodes before tuning individual subsystems | VM-only throughput cannot establish network throughput |

## Dependency candidates

This is a candidate inventory, not a tested dependency lockfile. Do not infer production suitability from presence upstream. Pin exact versions/revisions and review licensing, advisories and transitive native code after the foundation decision.

| Function | Candidate | Evaluation needed |
|---|---|---|
| Move | Matching `move-vm-runtime`, verifier, core types, VM types, compiler, framework and natives from one compatible pinned lineage | Cross-platform determinism, bytecode/gas compatibility, maintainer burden |
| Parallel execution | Starcoin v1 executor + MVHashMap; Rayon, crossbeam | VM adapter and property/differential tests; bounded retry memory |
| P2P | Upstream libp2p integration or a compatible pinned rust-libp2p release | Peer limits, transport/discovery behavior, upgrade cost |
| Runtime | Evaluate Tokio and the runtime requirements of selected libraries | Keep execution pools and queues bounded; avoid importing upstream service orchestration unnecessarily |
| Storage | RocksDB with bounded buffers/cache; authenticated Jellyfish state candidate | Native build support, recovery, compaction RAM and write amplification |
| Encoding | Serde plus canonical BCS lineage compatible with the VM | Reject noncanonical and oversized inputs; version formats |
| Crypto | Existing reviewed signature/hash libraries compatible with transaction types | Domain separation, malformed inputs, test vectors; no new primitives |
| PoW | Upstream CryptoNight-R; RandomX reference implementation/binding candidate | License inventory, FFI safety, arm64/x86 vectors and verification cost |
| RPC / metrics | jsonrpsee, tracing and Prometheus candidates | Bounded responses, authentication for local admin methods and observability |
| Verification | proptest, cargo-fuzz, Criterion, upstream integration harness | Properties that exercise consensus and state recovery |

Current upstream manifests were inspected for concrete dependencies. RandomX and replacement crate versions have not yet been source-qualified; do not add them to a workspace on the strength of this list.

## Prioritized work

1. **Complete component selection.** Produce a source-backed comparison of compatible Move VM/toolchain/framework lineages, executors, authenticated state/storage, P2P and PoW implementations. For each record exact source revision, license/attribution, maintenance and security evidence, dependencies, authority assumptions, integration work and 16 GB resource risks. Distinguish evidence from untested claims. Prioritize the Move/state/executor combination because those choices constrain one another. The user has authorized implementation; keep unqualified production choices explicitly provisional.
2. **Specify protocol invariants.** Fix encoding, chain identity, header/transaction validity, cumulative work, difficulty, timestamp, issuance, gas, replay and reorganization semantics. Write reviewable vectors before changing consensus code.
3. **Small functional network.** Startup/shutdown, persistence, transaction validation, CPU PoW, P2P, sync, RPC, competing branches and crash recovery. Integrate only selected components under the explicitly specified Nexium protocol after implementation is authorized.
4. **Sequential Move baseline.** Deployment/invocation, invalid bytecode, out-of-gas, aborts, framework upgrade restrictions and state-root reproduction on both architectures.
5. **Parallel equivalence.** Integrate selected executor only after baseline; compare worker counts, conflicts, retries, gas-prefix boundaries, events and commitments.
6. **Resource and overload controls.** Mining priority, queue/peer/cache caps, state retention, sustained overload and disk-full behavior.
7. **Performance acceptance.** Run staged load tests then the 24-hour milestone under the documented workload; publish raw evidence and failures.
8. **Privacy and advanced consensus.** Separate designs and security review after baseline acceptance.

Upstream implementations and tests provide evidence and reuse candidates; none automatically satisfies Nexium’s integration and acceptance gates.

## Main risks

- Mining, execution and proof verification compete for cores and memory bandwidth; reserving capacity may reduce hash rate substantially.
- Shared resources, sequence counters and fee destinations can serialize otherwise simple workloads; adversaries can force repeated speculative work.
- Ten-second blocks at 1,000 TPS average 10,000 transactions; propagation, validation bursts and state commits must fit that regime with variance headroom.
- Live state grows even with history pruning; storage economics and bounded transaction sizes need explicit treatment.
- New PoW networks have weak initial hash security; pool concentration, majority attacks, eclipse attacks and partitions remain threats.
- A fork inherits dependency/security maintenance and historical policy; an independent node inherits integration and consensus implementation risk.
- Snapshots, scarce archive sources and slow catch-up can erode practical independent verification.
- CryptoNight FFI and all native dependencies require review; successful compilation is not a memory-safety or consensus correctness guarantee.
- Shielded transfers add proving/wallet complexity, verification cost and larger transactions; existing cryptographic dependencies do not establish private payments support.

Unresolved inputs: exact minimum Mac/CPU, SSD capacity/endurance, sustained bandwidth, supported Linux architecture/distribution, acceptable synchronization time and deployment geography. Record initial benchmark assumptions explicitly while these are resolved.
