# Starcoin assessment

Assessment date: 2026-10-30 (Asia/Seoul). Historical source assessment: partial upstream macOS build/test evidence obtained; upstream runtime, Linux, security, and hardware qualification remain open. Nexium implementation began subsequently; see the repository README for its separate evidence and current scope.

## Current decision

The user has selected an **independent Nexium codebase with selective component reuse**, superseding the earlier conditional fork recommendation below. Starcoin remains a component and architectural reference. No wholesale fork is planned. The remaining assessment preserves the evidence and historical reasoning; component suitability must be evaluated separately from whole-node suitability. See [technical decisions](technical-decisions.md) for the active Phase 0 plan.

## Earlier recommendation and correction

**Follow-up correction (2026-10-30):** Further documentation research identified an important omission in the initial review. [v1.13.21 release notes](https://github.com/starcoinorg/starcoin/releases/tag/v1.13.21) introduce authority-signed block permits on Main at height 32,165,300. Inspection of v1.13.22 `types/src/block_permit.rs` and `chain/src/verifier/mod.rs` confirms this policy remains in the proposed base. Valid PoW alone does not satisfy that activated policy. Therefore v1.13.22 is **not a permissionless consensus base as shipped**. The evaluation recommendation below is conditional on explicitly excluding that authorization policy from Nexium and reviewing the affected validation, mining, networking and synchronization paths. Compare a pre-permit baseline and component reuse before choosing a fork. This materially increases the adaptation scope; the initial report understated it.

Starcoin is worth evaluating as a fork candidate. Prefer a **bounded evaluation of the conventional-chain v1.13.22 release** over immediately adopting the default branch or writing an entire node independently. Keep selected component reuse as the fallback. A production fork is not yet justified by measured evidence.

The conventional release already combines Rust, PoW, account-based Move execution, a parallel executor, authenticated state, persistence, mining, P2P, synchronization, RPC, and integration tests. Reimplementing all those boundaries creates substantial correctness and security work. Conversely, keeping an inherited system carries maintenance, historical compatibility, and governance assumptions that must be understood.

Adopt a fork only if it builds reproducibly on the supported platforms, can run an isolated new network without inherited control paths, and offers a practical route to the hardware target. Benchmark the unchanged upstream code first so modifications have an attributable baseline. Do not treat failure to reach 1,000 TPS immediately as automatic rejection: identify whether the limiting subsystem is tractable within the hardware budget.

## Source provenance

Public repository: <https://github.com/starcoinorg/starcoin>. A temporary research clone was downloaded; nothing was forked to a hosted account or imported as Nexium implementation.

| Reference inspected | Resolved commit | Observed characteristics |
|---|---|---|
| Default `dual-verse-dag` | `57559ea1a32c3af5a81d84668c9d4a40511991ed` | Commit dated 2026-08-13; workspace version 2.1.1; Rust 1.93.1; DAG and two VM trees |
| `v1.13.22` | `7c424e76af56efdbac669934dc21891f60d544eb` | Commit dated 2026-08-24; conventional-chain candidate; toolchain 1.66.1, manifest minimum 1.64 |
| `v2.0.14` | `b0a6e6f5dfb90b2db1b8b0da952608e4782a9275` | Commit dated 2025-04-11; DAG-era comparison; manifest package version is 2.0.7 despite tag name |

Sources should be pinned by commit, not inferred from tag names. Web branch listings returned stale snapshots during research; fetched Git refs and commit contents take precedence for this assessment. The last 12 fetched default-branch commits include human-authored VM metadata, synchronization, RPC, and benchmark changes as well as dependency updates. This establishes recent development, not maintainer responsiveness or a security support guarantee. No complete contributor concentration or issue-response audit was performed.

## Architecture findings

Paths below refer to the default commit unless a release is specified. Links are immutable source references.

| Area | Source observations | Implication |
|---|---|---|
| PoW | [`consensus/src/cn.rs`](https://github.com/starcoinorg/starcoin/blob/57559ea1a32c3af5a81d84668c9d4a40511991ed/consensus/src/cn.rs) uses CryptoNight-R; Argon, Keccak and dummy implementations also exist | RandomX is not the inspected integration; adopting it would require a consensus change and native binding review |
| Conventional chain | [`v1.13.22/chain/src/chain.rs`](https://github.com/starcoinorg/starcoin/blob/7c424e76af56efdbac669934dc21891f60d544eb/chain/src/chain.rs) accumulates difficulty and provides ancestor lookup; release difficulty code operates on chain headers | A much closer initial fit than DAG; full fork-choice and reorganization invariants still need executable tests |
| Move | `vm/` and `vm2/` use distinct pinned Starcoin Move revisions; framework and crypto also come from external repositories | Move is an ecosystem of VM, verifier, natives, gas, framework, compiler, and serialization contracts; dependencies cannot be swapped independently without review |
| State | `state/statedb`, `state/state-tree`, `commons/forkable-jellyfish-merkle`; account/resource access paths and sparse Merkle proofs | Account-based baseline with reusable authenticated-state concepts; pruning and crash durability need runtime verification |
| Persistence | Workspace RocksDB 0.21 with LZ4; storage config exposes block cache, write-buffer and WAL controls | Useful tuning points; configured cache size does not bound total process memory |
| Network | `network-p2p`, `network`, `network-rpc`, `sync`, `block-relayer`; default libp2p 0.56 versus release 0.50 | Considerable working infrastructure, with different dependency modernization costs across branches |
| Runtime | Actix services plus other async and parallel dependencies | Forking inherits service orchestration and lifecycle behavior, not just algorithms |
| Testing | `testsuite`, `simnet`, executor benchmarks, property tests, CI and release workflows | Valuable starting assets; workflow files alone do not prove successful runs or supported hardware |

## TurboSTM

Two operational observations warrant follow-up. Default `config/src/txpool_config.rs` sets a 4,096-transaction default count and derives a memory allowance from half of current free system memory unless overridden. That is a tunable bound, but it does not implement Nexium's proposed fixed 1 GB allowance or accommodate an average 10,000-transaction block without configuration review. Also, `SECURITY.md` still describes a pre-mainnet prototype and leaves disclosure procedure unfinished. That document is stale relative to current releases; establish the actual private reporting and patch process before relying on upstream security support.

The code contains substantive optimistic parallel execution, not only a website claim. Inspect `commons/parallel-executor/{src/executor.rs,src/scheduler.rs}`, `commons/mvhashmap`, and `vm/vm-runtime/src/parallel_executor/`. The v1.13.22 executor instead lives under `vm/parallel-executor` and has a smaller dependency surface than the current executor.

Transactions read versioned state; read-set validation detects invalidated dependencies and triggers reexecution. Scheduler and gas tracking coordinate progress and the transaction prefix. The VM adapter handles a `BlockRestart` by executing sequentially. The default branch adds delayed fields and resource-group materialization, increasing the scope of a correctness review. Generic property tests compare generated executions against baseline outputs, and additional mixed-resource tests exist. These are useful evidence of test intent, not proof of full VM equivalence or cross-platform determinism.

Selected reuse is plausible, especially the conventional executor, but it is not a dependency-free drop-in. It depends on local MVHashMap, logging and helper crates; VM adapters depend on Starcoin transaction, state, gas, and output types. Port tests with the implementation and compare status, writes, events, gas, and final state root against the sequential VM under varying worker counts.

## FlexiDAG

`flexidag/src/ghostdag/protocol.rs` implements selected-parent and GHOSTDAG-related machinery; the tree includes blue-work ordering, mergesets, reachability, consensus storage, and pruning-point services. The default consensus difficulty implementation reads DAG blue/red mergesets directly. DAG adoption therefore reaches consensus, storage, synchronization, mining, and execution ordering; it is not a later configuration toggle.

There is substantial implementation, but no security proof or independent audit was established in this review. Remaining questions include exact adversarial assumptions, parameter changes, partition behavior, transaction deduplication and order, pruning safety, and confirmation guarantees. Defer adoption until the conventional-chain milestones pass.

## License and inherited policy

The root LICENSE and workspace metadata declare Apache-2.0. Its text permits modified redistribution subject to its conditions; preserve the license, relevant attribution and any applicable NOTICE contents, and mark changed files. Trademark rights are not granted. See [upstream LICENSE](https://github.com/starcoinorg/starcoin/blob/57559ea1a32c3af5a81d84668c9d4a40511991ed/LICENSE) and [Apache's authoritative text](https://www.apache.org/licenses/LICENSE-2.0).

The root declaration is not a complete dependency license inventory. Bundled `consensus/cryptonight-rs/ext/oaes_lib.h` and `aesb.c` carry separate permissive redistribution notices. Inventory all vendored code, Git dependencies, native libraries and shipped artifacts before selecting a base; no complete license clearance is claimed here.

VM source includes enforced package-upgrade handling and historical main/barnard height conditions, with special handling for the genesis address. Development configurations include genesis key material, while several public-network configurations do not. These observations do **not** establish that current Starcoin has a universal emergency key. They establish that a Nexium fork must trace capabilities, framework authorities and genesis configuration explicitly rather than assume its no-admin-key requirement is inherited.

The [v1.13.22 release notes](https://github.com/starcoinorg/starcoin/releases/tag/v1.13.22) describe historical replay compatibility fixes. This is both evidence of maintenance and a warning that historical validation semantics have operational consequences. A new genesis should not inherit Starcoin's network identifiers, bootnodes, allocations or historical exceptions accidentally.

## Build and performance evidence

### Published minimum hardware guidance

In a [2023-11-29 reply to the upstream FlexiDAG pre-mining guide](https://github.com/starcoinorg/starcoin/discussions/3983), maintainer `sanlee42` stated that solo mining required an x86 computer, one CPU core and 1 GB RAM. This is historical DAG test-network guidance, not a verified specification for today's Main/Vega full-chain synchronization or sustained heavy workloads. The reply gives no disk capacity or bandwidth minimum. The official Cookbook installation and Vega setup pages reviewed do not provide a complete current numerical hardware table. Do not interpret build troubleshooting examples with 2 GB WSL memory as node requirements. These findings support investigating consumer hardware, but establish neither current 1 GB operation nor our 16 GB / 1,000 TPS target.

Host observed: macOS arm64; Homebrew rustc/cargo 1.93.1; clang, CMake and protoc available; rustup unavailable. No Linux execution environment was established.

- `cargo metadata --locked --offline --format-version 1 --no-deps` succeeded on the default checkout. This validates workspace manifest discovery only.
- `cargo check --locked --offline -p starcoin-consensus` stopped at an uncached Git dependency (`merlin`), before compilation. This is an environment limitation, not a source incompatibility finding.
- A network-enabled default-branch `cargo check --locked -p starcoin-consensus` passed on this host, including RocksDB, DAG and both VM dependency paths. This is a development-profile package check, not a linked node build. Duration reported by Cargo: 6m 15s.
- The same check on v1.13.22 using installed Rust 1.93.1 encountered E0282 in locked dependency `time 0.3.19`. The release's pinned Rust 1.66.1 was not installed or tested. This establishes a modern-toolchain obstacle, not failure on its intended compiler. No dependency source or lockfile was patched.
- Default-branch `cargo test --locked -p cryptonight-rs --lib` passed four existing hash-vector tests, with one ignored thread-spawning test. This is local arm64 test evidence only.
- Commands, scope and diagnostics are recorded in [build-check](research/build-check.md).
- Apple Silicon clang syntax-checking of the upstream `slow-hash.c` succeeded. This checks one native translation unit only, not linkage, hash correctness or whole-node support.
- Default release workflows list macOS and Ubuntu runners. This does not independently establish native arm64 binary compatibility or passing CI.
- No node startup, multi-node test, benchmark reproduction, hardware qualification, or security audit has passed in this assessment.

The default [`sync/starcoin-execute-bench/README.md`](https://github.com/starcoinorg/starcoin/blob/57559ea1a32c3af5a81d84668c9d4a40511991ed/sync/starcoin-execute-bench/README.md) distinguishes execution-time and block timing metrics. Its hardware description and example results do not establish the project milestone. CI invokes short simple-transfer runs, including fixed-block-time settings. Treat these as regression tooling to adapt, not evidence of 24-hour multi-node throughput with competitive CPU mining. Website throughput multipliers remain unverified.

## Historical fork comparison (superseded by selective-reuse decision)

| Option | Benefit | Principal cost | Present position |
|---|---|---|---|
| Fork v1.13.22 | Most direct integrated conventional PoW + Move baseline | Old toolchain/dependencies, inherited framework and historical policy | Leading candidate for a measured evaluation |
| Fork default DAG branch | Newer dependencies and ongoing features | DAG plus dual-VM scope conflicts with initial simplicity | Do not choose for Phase 1 |
| Reuse selected components | Smaller inherited policy surface | Adapter work and ownership of node integration correctness | Preferred fallback |
| Implement all subsystems independently | Maximum architectural control | Largest implementation and security burden | Avoid without evidence that reuse fails |

Before committing: build and start both platforms; run an isolated conventional PoW network; validate new-genesis authority boundaries; reproduce sequential/parallel correctness; measure mining plus validation memory/CPU; inventory dependency licenses and advisories; and estimate ongoing patch maintenance. Record a final go/no-go with results. Research conclusions are useful now, but Phase 0 hardware/build qualification is still incomplete.
