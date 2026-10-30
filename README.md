# Nexium

An independent Rust Layer 1 development prototype with an account/resource-based Move direction. This repository contains an original, minimal PoW node and selected upstream libraries, not a Starcoin fork.

## Current milestone

- Signed Ed25519 public transfers, nonces, network replay protection and expiry.
- Sequential native transfer execution and deterministic state commitments.
- CPU-mined development SHA3 PoW, integer difficulty adjustment and cumulative-work fork choice.
- Side-branch validation, reorganization rollback and displaced-transaction reconsideration.
- Atomic SQLite persistence, exclusive database ownership and restart recovery.
- Bounded mempool and local HTTP peer polling with block locator synchronization.
- A separately pinned Move VM, verifier and compiler compatibility test with 32-byte addresses.

**Local development only.** The genesis allocation uses a publicly known funding key. SHA3 PoW is a protocol test harness, not the selected CPU-friendly mining algorithm. Networking currently accepts numeric loopback endpoints only. Move contract deployment/invocation, gas schedules, framework, libp2p discovery, pruning, Merkle proofs, parallel execution and privacy are not implemented. This is not a performance or production-security milestone.

## Build and verify

Rust 1.93.1, a C compiler for bundled SQLite, and Git/network access for pinned dependencies are needed. `Cargo.lock` is included in the source tree; use `--locked` for subsequent builds. No Starcoin binary or running service is needed.

```sh
cargo build -p nexium-node --locked
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/devnet_smoke.py
```

The smoke test starts three separate local processes, propagates and mines a transfer, kills and restarts a follower, verifies catch-up and identical canonical state, and cleans up all processes and temporary files. It is not a test on three physical machines.

## Run two local nodes

Create separate data directories and a development manifest:

```sh
mkdir -p .devnet
cargo run -p nexium-node --locked -- init --out .devnet/genesis.json
cargo run -p nexium-node --locked -- keygen --out .devnet/miner.key
```

Copy the printed address into `<MINER_ADDRESS>`. In separate terminals:

```sh
target/debug/nexium run --genesis .devnet/genesis.json --db .devnet/a.sqlite --listen 127.0.0.1:9840 --peer http://127.0.0.1:9841 --mine-to <MINER_ADDRESS>
```

```sh
target/debug/nexium run --genesis .devnet/genesis.json --db .devnet/b.sqlite --listen 127.0.0.1:9841 --peer http://127.0.0.1:9840
```

Use `nexium status --rpc http://127.0.0.1:9840` to inspect the head. Stop with Ctrl-C; use the same manifest/database to resume. Random key files are created with Unix mode 0600 and existing files are not overwritten. The `transfer` command signs locally with a key file; `--help` describes its arguments. The smoke test demonstrates funding from the explicitly insecure development allocation.

Each genesis parameter contributes to the network identity. Nodes must share exactly the same manifest. A database rejects a different manifest; use a fresh development database for a different network.

## HTTP interface

| Method/path | Operation |
|---|---|
| `GET /status` | Network, canonical tip, height, state root, cumulative work and pool occupancy |
| `GET /account/{32-byte hex address}` | Canonical spendable balance and next nonce |
| `GET /transactions` | Bounded pending native transfers |
| `POST /transactions` | Validate and admit a signed transfer; does not promise inclusion |
| `POST /blocks` | Independently validate/store a block; possibly change canonical tip |
| `POST /sync` | Match a block locator and return at most 16 canonical blocks |

Transport uses JSON; signatures, hashes and persisted protocol objects use canonical BCS. JSON byte arrays are explicit arrays of integers. Errors use non-success HTTP status codes. These routes are a development HTTP API, not JSON-RPC 2.0.

See [development protocol](docs/development-protocol.md), [dependency decisions](docs/dependency-baseline.md), and [benchmark plan](docs/benchmark-plan.md).
