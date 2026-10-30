# Initial dependency baseline

Status: development implementation authorized; first independent prototype. All direct dependencies have exact manifest versions; `Cargo.lock` fixes the transitive graph. This is a bounded integration baseline, not a security audit or final production selection.

## Move decision

Use one account/resource VM lineage from `starcoinorg/move` at commit `cc98ac02caa73e64e6c8b781c61de356b45801c4`, with `address32` enabled consistently. Pin VM runtime, core types, binary format, verifier, VM types and compiler to that same revision. Compiler and VM types are currently adapter test dependencies. [Upstream source](https://github.com/starcoinorg/move/tree/cc98ac02caa73e64e6c8b781c61de356b45801c4).

Reason: previous source research established an account-based integration using this revision, and the isolated runtime/verifier can be reused without Starcoin's node, framework, chain controls or dual-VM integration. This lets us test the consensus-independent adapter immediately. Selection does **not** establish that this older fork is the best-maintained Move distribution. Before accepting untrusted contracts, compare current account-based Move lineages and advisories, identify bytecode/feature compatibility requirements and qualify the native/gas/framework surface. Changing this dependency remains possible before a public contract interface is committed.

No existing blockchain framework is selected. Importing an entire framework would also import its native function, gas, address and authority assumptions. The first adapter has no native registry, framework modules, publish endpoint or on-chain Move transaction type. Recreate the VM when storage rolls back or module cache coherence is uncertain, as required by the upstream VM's documented loader behavior.

## Other components

| Boundary | Baseline | Reason and limitation |
|---|---|---|
| Serialization | BCS 0.1.5 + Serde 1.0.228 | Canonical typed consensus encoding; JSON only for HTTP/files |
| Hashing/signatures | SHA3 0.10.8, ed25519-dalek 2.2.0 | Established primitives; strict signature validation, network domain separation; no original cryptography |
| Entropy | rand 0.8.5 OS RNG | Local random development keys; public fixture key explicitly separate |
| Persistence | rusqlite 0.32.1, bundled SQLite | Atomic transaction/recovery behavior with small initial integration; full snapshots are temporary, no throughput claim |
| Runtime | Tokio 1.51.1 | Bounded configured peer tasks, separate blocking ledger/miner work |
| HTTP | Axum 0.8.8, reqwest 0.12.28 | Development API and local peer exchange; not final P2P/discovery |
| CLI | Clap 4.6.0 | Explicit local node/key/transfer commands |
| Tests | tempfile 3.27.0, standard-library Python smoke harness | Isolated databases/processes; reproducible functional checks |

No Starcoin PoW implementation has been copied. The first SHA3 miner exercises protocol validity and fork handling. Production CPU PoW, authenticated Merkle state, storage backend, parallel executor and P2P implementation remain separate decisions. SQLite is selected for the first development milestone only; the benchmark plan will determine whether RocksDB or another backend is appropriate for the resource target.

## Reuse and maintenance record

The root source license is Apache-2.0. Upstream Move crates declare Apache-2.0 and retain their licenses in dependency sources. Direct registry dependencies and all transitives still require an artifact-specific inventory before distribution; a root license does not replace their obligations. Cargo may fetch transitive Git dependencies declared by Move core types (including BCS and OpenRPC); they are fixed in the lockfile and must be included in maintenance and license review. No claim of complete dependency license clearance is made.

Read upstream primary documentation for the chosen API versions: [ed25519-dalek](https://docs.rs/ed25519-dalek/2.2.0/ed25519_dalek/), [rusqlite](https://docs.rs/rusqlite/0.32.1/rusqlite/), [Axum](https://docs.rs/axum/0.8.8/axum/). Build and runtime evidence for Nexium belongs in its own milestone report, not the earlier upstream Starcoin build record.
