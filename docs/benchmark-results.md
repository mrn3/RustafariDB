# Benchmark Results: RustafariDB vs MySQL / PostgreSQL

## Test configuration

- **Workload**: Sysbench-style OLTP read-only (point SELECTs + range SELECTs on `sbtest`).
- **Scale**: 10,000 rows, 1 table, 10-second run.
- **Per transaction**: 10 point SELECTs (`WHERE id = ?`) + 1 range SELECT (`WHERE id >= ? AND id <= ?`), i.e. 11 queries/transaction.

---

## RustafariDB (measured)

Runs in-process, single-threaded, release build.

### With B-tree index on `id` (current)

Point and range lookups use the primary-key index; no full table scan.

| Metric | Value |
|--------|--------|
| **Duration** | 10.00s |
| **Transactions** | 72,492 (**7,249 tps**) |
| **Queries** | 797,412 (**79,740 qps**) |
| **Latency p50 / p95 / p99** | 0.13 ms / 0.15 ms / 0.17 ms |

### Before index (full table scan, baseline)

| Run | TPS | QPS | Latency p50 (ms) |
|-----|-----|-----|------------------|
| 1   | 48.9 | 537.5 | 20.46 |
| 2   | 47.5 | 522.6 | 20.94 |

---

## PostgreSQL (measured)

Sysbench oltp_read_only, same scale (10k rows, 1 table, 10s, 1 thread). Database: `attask`, host: localhost:5432.

| Metric | Value |
|--------|--------|
| **Transactions** | ~4,038 (**404 tps**) |
| **Queries** | ~64,608 (**6,459 qps**) |
| **Latency min/avg/max** | 1.69 ms / 2.48 ms / 12.85 ms |
| **Total time** | 10.00s |

## MySQL (not run)

No MySQL server was used in this run. To add MySQL, follow [docs/benchmark.md](benchmark.md) with your MySQL credentials.

---

## Performance analysis and comparison

### RustafariDB

- **Execution model**: Single process, no client/server. The benchmark calls the executor directly, so there is no network or serialization overhead.
- **Storage**: In-memory row store; no disk I/O during the run.
- **Execution**: Currently **full table scan** for every query (no B-tree index used on `id` in the benchmark path). Each point SELECT and range SELECT scans the whole table and then applies the filter.
- **Interpretation**: ~530 qps with full scans on 10k rows is consistent with doing 11 full scans per transaction (10 point + 1 range) and ~48 tps. Latency is dominated by scan + filter cost.

### Direct comparison: RustafariDB (with index) vs PostgreSQL

| Metric | RustafariDB (indexed) | PostgreSQL | Ratio (RustafariDB / PG) |
|--------|------------------------|------------|---------------------------|
| **Queries/sec** | **79,740 qps** | 6,459 qps | **~12× higher** |
| **Transactions/sec** | **7,249 tps** | 404 tps | **~18× higher** |
| **Latency (p50 / avg)** | **0.13 ms** | 2.48 ms | **~19× lower** |

- **RustafariDB** now uses a B-tree index on `id`: point lookups (`id = ?`) and range lookups (`id >= ? AND id <= ?`) use the index instead of a full table scan. Execution is in-process (no client/server), so throughput is very high.
- **PostgreSQL** uses primary key and indexes over the wire (sysbench client → server), so it has protocol and connection overhead; its numbers are still strong for a networked DB.
- RustafariDB’s indexed path is much faster than its previous full-scan baseline (~150× more qps, ~160× lower latency) and exceeds PostgreSQL in this same-workload, single-thread test.

### Suggested next steps

1. **Run MySQL and PostgreSQL** locally using [docs/benchmark.md](benchmark.md) and record their qps/tps and latency.
2. **Compare** those numbers to the RustafariDB table above (same scale: 10k rows, 10s, 1 thread).
3. **In RustafariDB**: Implement use of the existing B-tree index for `WHERE id = ?` and `WHERE id >= ? AND id <= ?`, then re-run the benchmark to see the impact of indexed vs full-scan execution.

---

## How to reproduce

```bash
# RustafariDB (from repo root)
cargo run -p rustafari-bench --release -- --table-size 10000 --time 10
```

For MySQL/PostgreSQL, use the sysbench commands in [docs/benchmark.md](benchmark.md) with `--table-size=10000`, `--time=10`, `--threads=1`.
