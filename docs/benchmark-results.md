# Benchmark Results: RustafariDB vs MySQL / PostgreSQL

## Test configuration

- **Workload**: Sysbench-style OLTP read-only (point SELECTs + range SELECTs on `sbtest`).
- **Scale**: 10,000 rows, 1 table, 10-second run.
- **Per transaction**: 10 point SELECTs (`WHERE id = ?`) + 1 range SELECT (`WHERE id >= ? AND id <= ?`), i.e. 11 queries/transaction.

---

## RustafariDB (measured)

Runs in-process, single-threaded, release build.

| Run | Duration | Transactions | TPS | Queries | QPS | Latency p50 (ms) | p95 (ms) | p99 (ms) |
|-----|----------|--------------|-----|---------|-----|------------------|----------|----------|
| 1   | 10.01s   | 489          | 48.9 | 5,379   | **537.5** | 20.46 | 22.47 | 23.05 |
| 2   | 10.02s   | 476          | 47.5 | 5,236   | **522.6** | 20.94 | 23.09 | 29.75 |

**Summary**: ~**530 qps**, ~**48 tps**, ~**20–21 ms** p50 latency (10k rows, in-process).

---

## PostgreSQL (measured)

Sysbench oltp_read_only, same scale (10k rows, 1 table, 10s, 1 thread). Database: `attask`, host: localhost:5432.

| Metric | Value |
|--------|--------|
| **Transactions** | 3,970 (396.93 tps) |
| **Queries** | 63,520 (6,350.87 qps) |
| **Latency min/avg/max** | 1.60 ms / 2.52 ms / 9.58 ms |
| **Total time** | 10.0016s |

## MySQL (not run)

No MySQL server was used in this run. To add MySQL, follow [docs/benchmark.md](benchmark.md) with your MySQL credentials.

---

## Performance analysis and comparison

### RustafariDB

- **Execution model**: Single process, no client/server. The benchmark calls the executor directly, so there is no network or serialization overhead.
- **Storage**: In-memory row store; no disk I/O during the run.
- **Execution**: Currently **full table scan** for every query (no B-tree index used on `id` in the benchmark path). Each point SELECT and range SELECT scans the whole table and then applies the filter.
- **Interpretation**: ~530 qps with full scans on 10k rows is consistent with doing 11 full scans per transaction (10 point + 1 range) and ~48 tps. Latency is dominated by scan + filter cost.

### Direct comparison: RustafariDB vs PostgreSQL (same workload)

| Metric | RustafariDB | PostgreSQL | Ratio (PG / RustafariDB) |
|--------|-------------|------------|---------------------------|
| **Queries/sec** | ~530 qps | **6,351 qps** | ~12× |
| **Transactions/sec** | ~48 tps | **397 tps** | ~8× |
| **Latency (avg / p50)** | ~21 ms | **2.52 ms** | ~8× lower |

- **PostgreSQL** uses primary key and secondary index on `id` for point and range lookups (index seeks), so it does very little work per query.
- **RustafariDB** currently does a **full table scan** for every query (no index used on `id`), so each of the 11 queries per transaction scans all 10k rows.
- The ~8–12× gap is expected. Adding and using a B-tree index on `id` in RustafariDB’s execution path should narrow this gap.

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
