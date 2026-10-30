# Upstream build check record

Date: 2026-10-30 (Asia/Seoul). Historical upstream research record, before the subsequent Nexium implementation was authorized. All compilation in this record used unmodified temporary upstream sources. See the repository README for the later Nexium prototype.

## Environment

- Host: Darwin arm64.
- Installed compiler: `rustc 1.93.1 (01f6ddf75 2026-02-11) (Homebrew)`.
- Cargo: `cargo 1.93.1 (Homebrew)`.
- clang, CMake and protoc found. rustup not found.
- Temporary default checkout: `/private/tmp/nexium-starcoin-assessment`.
- Conventional release exported from fetched Git tag to `/private/tmp/nexium-starcoin-v1.13.22`.
- Temporary isolated dependency cache: `/private/tmp/nexium-starcoin-cargo`.
- `CARGO_BUILD_JOBS=2` for network-enabled package checks and mining tests.
- No global toolchain installation, source patch or lockfile modification was performed.

## Results

| Check | Result | Limit |
|---|---|---|
| Default `cargo metadata --locked --offline --format-version 1 --no-deps` | Passed | Workspace manifests only |
| Default offline `cargo check --locked --offline -p starcoin-consensus` before downloads | Could not load uncached `merlin` Git dependency | Not a compiler error |
| Default network-enabled `cargo check --locked -p starcoin-consensus` | Passed; Cargo reported 6m 15s | Development check; no node link or startup |
| v1.13.22 `cargo check --locked -p starcoin-consensus` on installed 1.93.1 | Failed in `time 0.3.19`, E0282 | Pinned release toolchain 1.66.1 not tested |
| Default `clang -fsyntax-only -O3 -ffast-math -fexceptions -std=gnu99 -Iconsensus/cryptonight-rs/ext consensus/cryptonight-rs/ext/slow-hash.c` | Passed | One native translation unit, no linked vectors |
| Default `cargo test --locked -p cryptonight-rs --lib` | Four passed, zero failed, one ignored | Current arm64 development-profile hash tests only |

The first mining test attempt used offline mode and stopped at missing `bencher 0.1.5`; the network-enabled retry downloaded the dependency and passed. Native archive tooling emitted an `ar: illegal option -- D` warning, but the final package check and mining tests succeeded. Preserve and investigate that diagnostic when standardizing the toolchain.

The passing mining tests were `test_slow4`, `test_amd_ryzen`, `test_main_block2_jh_on_ubuntu22`, and `test_barnard_block3_on_ubuntu22`. Names refer to upstream regression cases; all four ran on this macOS arm64 host. `test_with_spawn` is ignored upstream and loops spawning threads; it was not enabled.

The conventional release error was:

```text
error[E0282]: type annotations needed for `Box<_>`
time-0.3.19/src/format_description/parse/mod.rs:83:9
error: could not compile `time` (lib) due to 1 previous error
```

Upstream sources remained unchanged. A follow-up should first reproduce the pinned legacy toolchain, then evaluate a separately recorded minimal dependency modernization patch if selecting this fork. Neither action is necessary to establish the present source-based recommendation, and neither has been performed.

## Reproduction

Fetch and resolve the exact commits recorded in [the assessment](../starcoin-assessment.md). In each corresponding source directory, run:

```sh
CARGO_HOME=/private/tmp/nexium-starcoin-cargo CARGO_BUILD_JOBS=2 cargo check --locked -p starcoin-consensus
```

For the default-branch mining tests:

```sh
CARGO_HOME=/private/tmp/nexium-starcoin-cargo CARGO_BUILD_JOBS=2 cargo test --locked -p cryptonight-rs --lib
```

These commands use the installed Homebrew compiler in this environment; they do not reproduce the v1 release's intended toolchain. Future results must record the actual compiler selected.

## Unverified

Full executable build, node lifecycle, mining/block propagation, new genesis, restart recovery, Move contract runtime behavior, Linux build/run, cross-platform consensus equivalence, memory ceiling and all TPS milestones. A Docker client was present, but sandbox access to its daemon socket was denied; no Linux runtime test was performed. Build timing is not a throughput measurement.
