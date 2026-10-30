# Reproducible benchmark plan

Date: 2026-10-30. This is a protocol for future measurements; no performance result is claimed.

## Test identity

For every run record source SHA and patches, lockfile checksum, compiler and native toolchain, OS/kernel, CPU/cores, physical RAM, SSD model/free space/filesystem, network interface, bandwidth/latency/loss, topology, genesis, difficulty parameters, gas schedule, transaction/block limits, pruning, all cache/queue limits and miner worker/priority configuration. Archive launch commands and sanitized configuration. Separate load generators from validating nodes.

Use physical 16 GB machines for the acceptance run, including Apple Silicon macOS and Linux x86_64. A memory-limited process on a larger host is useful for development but does not establish equivalent cache, CPU, disk or OS behavior. Report GB versus GiB explicitly; monitor system memory pressure, compression and swap as well as process RSS.

## Stages

1. Reproduce upstream microbenchmarks and an isolated node; publish failures as well as successes.
2. Compare sequential and parallel VM with identical signed inputs and initial state. Sweep 1, 2, 4 and available reserved execution workers while mining is disabled and enabled.
3. Establish a three-node functional network, then use at least five separately hosted full validating nodes for the acceptance run. Each runs CPU mining; report actual PoW settings and shares. Containers on one machine are not independent hardware evidence.
4. Apply offered loads of 100, 250, 500, 750 and 1,000 TPS, with warm-up and at least 30 measured minutes per viable step. Increase further until backlog or resource pressure reveals capacity. Repeat selected short runs at least three times.
5. Run the target for 24 measured hours, excluding account funding and warm-up. Use sufficient steady offered-load headroom to maintain 1,000 committed successful transactions/s over the full interval; specify a rolling ten-minute throughput tolerance before the run. Do not demand 1,000 TPS every second on a stochastic ten-second PoW chain.
6. Run partition/rejoin, competing-miner reorg, process kill/restart, slow peer, invalid-data, disk-pressure and sustained-overload scenarios separately. Keep fault periods separate from the normal acceptance interval unless explicitly testing throughput during faults.

Initial topology assumptions: record a low-latency LAN baseline, then a latency-shaped multi-host profile (e.g. 50/100/200 ms RTT and a declared home-network bandwidth cap). These are scenarios, not a claim to model the global network. Include a slow-but-supported node and demonstrate catch-up with mining active.

## Workloads

| Workload | Purpose |
|---|---|
| Independent public transfers, prefunded accounts | Best-case execution and network/storage pipeline |
| Transfers with shared senders/recipients | Sequence, balance and fee contention |
| Move calls with declared complexity and distinct resources | Representative contract costs |
| Shared-resource Move calls | Conflict/retry and sequential fallback costs |
| Aborts, out-of-gas, invalid signatures and oversized inputs | Validation cost and overload behavior; never count as successful TPS |
| Account/resource creation versus reused state | Separate live-state growth from hot-cache throughput |
| Shielded transfers, later | Proof/transaction size, verification latency and memory; separately reported TPS |

Publish signed transaction size distribution, account count, state size, random seed and transaction-generation rules. Include disk-resident state larger than the configured state cache; report cold and warm runs separately. Avoid a single repeated transaction or a tiny hot state as the only result.

## Counting and correctness

Primary throughput is unique successful user transactions included in the eventual canonical chain divided by wall-clock measurement seconds. Also report offered, admitted, included, executed, failed, expired, evicted, duplicate and orphaned counts. Count each user transaction once; do not inflate results with block metadata, funding or retries. Report gas and bytes/s alongside TPS.

Use a fixed, declared confirmation depth for reporting confirmation latency, explicitly probabilistic. After stopping load, drain and reconcile against a common canonical block hash after additional confirmation blocks. Every node must agree on state/transaction commitments at the same block, not merely report its latest root. Retain evidence sufficient to detect later reorganizations of the reporting boundary.

Durably log submitted transaction identifiers and RPC admission outcomes. Reconcile each admitted transaction to inclusion or a documented terminal outcome, including replacement, expiration, rejection and reorg handling. Never erase dropped admitted transactions from results. Define whether admission promises eventual inclusion or only bounded pool acceptance; the latter must be visible in the API.

## Metrics and acceptance

- At least 1,000 successful canonical TPS averaged over 24 measured hours for the named public workload. Other workload classes receive separate results.
- No state divergence; compare sequential reference and parallel outputs across supported architectures and worker counts.
- All supported nodes remain within the machine's 16 GB physical-memory capacity with acceptable system headroom and no sustained swap dependence. Collect process/native allocations, OS pressure and peak memory during compaction and reorgs.
- Bounded mempool bytes/count, orphan cache, peer queues, speculative execution and database buffers. Plot backlog slope and p50/p95/p99 admission-to-inclusion latency; a capped pool that drops traffic is not proof of sustainable throughput.
- Active non-dummy CPU PoW throughout the acceptance interval, with hash rate, worker count, utilization and validation delays reported. Distinguish permissive test difficulty from realistic validation/block costs.
- Stable synchronization; block propagation and validation latency, stale-block rate and reorg depths remain documented and bounded under the declared scenario.
- Measure disk growth, live-state growth, write amplification/compaction and network ingress/egress. Report archival versus pruned behavior.
- Crash/restart restores durable committed state and resynchronizes without manual database repair; demonstrate in a separate fault run and verify persisted commitments.

Keep machine-readable time series, per-node logs, load-generator ledger, transaction outcomes, state-root checkpoints and an environment manifest. Publish both the full interval and rolling windows; executor-only or fixed-time synthetic results are supplementary.

## Capacity arithmetic

1,000 TPS for 24 hours is 86.4 million transactions. At an illustrative 300 signed bytes per transaction this is 25.92 GB/day of raw transactions before blocks, indexes, state and propagation overhead; at 1 KB it is 86.4 GB/day. These are decimal arithmetic examples, not measured Starcoin sizes. Raw single-copy transaction payload bandwidth at 300 bytes is 2.4 Mbit/s; actual P2P traffic can be substantially larger. Measure it.

With ten-second target blocks, average capacity must exceed 10,000 transactions/block to accommodate load and stochastic gaps. Report actual block bytes and gas. A full-node catch-up test must show replay speed greater than ongoing chain growth and give time-to-tip on the declared disk/network hardware.
