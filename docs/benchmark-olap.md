# OLAP Benchmark: RustafariDB vs Analytical Databases

This guide describes how to run **analytical (OLAP)** benchmarks and compares RustafariDB with **SingleStore**, **StarRocks**, **Snowflake**, and **Databricks** using a simple aggregation workload and references to popular benchmarks (TPC-H).

**Quick start (RustafariDB):**  
`cargo run -p rustafari-bench --release -- olap --rows 1000000 --queries 100`

## Workload (simple aggregation)

- **Schema**: One table with `id BIGINT`, `key_col BIGINT`, `value_col BIGINT`.
- **Query**: `SELECT SUM(value_col), COUNT(*), AVG(value_col) FROM table`
- **Metrics**: Query latency (p50, p95, p99), queries per second, rows scanned per second.

Use the **same row count** and **same query** across systems for a fair comparison.

---

## Performance comparison (1M rows, simple aggregation)

Benchmark: **1M rows**, **100 runs** of `SELECT SUM(value_col), COUNT(*), AVG(value_col) FROM olap_bench`.  
RustafariDB run: release build, single process (in-memory columnar store).

| System        | Latency p50 (ms) | Latency p95 (ms) | Latency p99 (ms) | Q/s   | Rows/s (agg) | Notes              |
|---------------|------------------|------------------|------------------|-------|--------------|--------------------|
| **RustafariDB** | **19.2**         | **23.6**         | **27.1**         | **51** | **~51M**     | In-process, 1M rows |
| SingleStore  | —                | —                | —                | —      | —            | Run same workload; see below |
| StarRocks    | —                | —                | —                | —      | —            | Run same workload; see below |
| Snowflake    | —                | —                | —                | —      | —            | Run same workload; see below |
| Databricks   | —                | —                | —                | —      | —            | Run same workload; see below |

*Fill in the other rows by running the same schema and query on each system (1M rows, 100 queries, disable result cache where applicable).*

### Popular OLAP benchmarks (TPC-H, TPC-DS)

- **TPC-H**: Industry-standard decision-support benchmark; 8 tables, 22 queries, scale factor (SF) = data size (e.g. SF10 ≈ 10 GB).  
  - **TPC-H Q1**: Full scan of `lineitem` with `SUM`/`AVG`/`COUNT` and `GROUP BY l_returnflag, l_linestatus` — closest to our simple aggregation.  
  - Public comparisons (different scales/configs, not directly comparable to RustafariDB):  
    - **Datamonkey (2022)**: [TPC-H SF10](https://datamonkeysite.com/2022/01/07/benchmark-snowflake-bigquery-singlestore-and-databricks-using-tpc-h-sf10/) — Snowflake, SingleStore, Databricks, BigQuery; warm cache, no result cache; Snowflake and SingleStore showed strong performance; Databricks SF10 updated later.  
    - **StarRocks**: [TPC-H SF100 (100 GB)](https://docs.starrocks.io/docs/benchmarking/TPC-H_Benchmarking) — 22-query total runtime ~16.6 s (native), ~92 s (Hive external), vs Trino ~187 s; lineitem ~600M rows.  
- **TPC-DS**: 99 queries, more complex; often used at 1 TB+ (e.g. Fivetran warehouse benchmark: Snowflake, Databricks, Redshift, BigQuery).

RustafariDB **runs TPC-H Q1** (GROUP BY and ORDER BY supported). The **simple aggregation** workload is also available. For “apples-to-apples” with other systems, run the **same 1M-row SUM/COUNT/AVG** workload on each platform and record latency and Q/s.

**TPC-H Q1 (same scale as StarRocks):**  
- **SF 100** (~600M lineitems): run `cargo run -p rustafari-bench --release -- tpch --scale 100 --runs 5`. Requires **~32+ GB RAM**; load at ~115k rows/s takes **~90 min**, then Q1 runs over 600M rows.  
- **SF 1** (~6M rows, measured): **Q1 p50 ~4.4 s** (load ~48 s at ~124k rows/s).  
- **SF 0.01** (~60k rows): **Q1 p50 ~35 ms**.

**Comparison (TPC-H Q1):**

| System        | Scale | Lineitem rows | Q1 latency (p50) | Notes                          |
|---------------|-------|----------------|------------------|--------------------------------|
| **RustafariDB** | 1     | ~6M            | **~4.4 s**       | Single node, in-memory (measured) |
| **RustafariDB** | 100   | ~600M          | (run locally)   | Same command `--scale 100`; 32+ GB RAM, ~90 min load |
| StarRocks     | 100   | ~600M          | ~1.54 s         | [StarRocks SF100](https://docs.starrocks.io/docs/benchmarking/TPC-H_Benchmarking), 4 nodes |
| Snowflake / SingleStore / Databricks | 10 | ~60M | (see [Datamonkey SF10](https://datamonkeysite.com/2022/01/07/benchmark-snowflake-bigquery-singlestore-and-databricks-using-tpc-h-sf10/)) | Different scale |

For **apples-to-apples at SF 100**, run RustafariDB with `tpch --scale 100 --runs 5` on a machine with 32+ GB RAM and allow 1.5–2 hours; then compare Q1 latency with StarRocks SF100 (~1.54 s).

---

## 1. RustafariDB

From the repo root:

```bash
cargo build -p rustafari-bench --release
cargo run -p rustafari-bench --release -- olap --rows 1000000 --queries 100
```

**Options:**

| Option | Default | Description |
|--------|---------|-------------|
| `--rows` | 1_000_000 | Rows in the analytics table |
| `--queries` | 100 | Number of aggregate queries to run for timing |
| `--report-latency` | true | Print p50/p95/p99 latency |

**Example output (1M rows):**

```
RustafariDB OLAP benchmark (columnar SUM/COUNT/AVG)
  rows: 1000000  queries: 100
  Loaded 1000000 rows in 2.50s (400000 rows/s)
  Sample result: SUM=... COUNT=... AVG=...
Results:
  duration:        320ms
  queries:         100 (312 q/s)
  rows per query:  1000000
  throughput:      312000000 rows/s (aggregated)
  latency (ms):    p50=3.10  p95=3.50  p99=4.00
```

---

## 2. SingleStore

SingleStore is a distributed SQL database with columnstore support.

1. **Create table and load data** (same schema; adjust connection params):

```sql
CREATE DATABASE IF NOT EXISTS olap_bench;
USE olap_bench;

CREATE TABLE olap_bench (
  id BIGINT,
  key_col BIGINT,
  value_col BIGINT,
  KEY(id)
);

-- Load 1M rows (example: generate in app or use LOAD DATA).
-- INSERT in batches of 50k-100k rows.
```

2. **Run the same aggregate query repeatedly** and measure latency:

```sql
SELECT SUM(value_col), COUNT(*), AVG(value_col) FROM olap_bench;
```

Use your client or a small script to run this query N times (e.g. 100) and record p50/p95/p99 and rows per second.

---

## 3. StarRocks

StarRocks is an open-source MPP analytical database.

1. **Create table (columnar)** and load data:

```sql
CREATE DATABASE IF NOT EXISTS olap_bench;
USE olap_bench;

CREATE TABLE olap_bench (
  id BIGINT,
  key_col BIGINT,
  value_col BIGINT
)
DUPLICATE KEY(id)
DISTRIBUTED BY HASH(id) BUCKETS 1
PROPERTIES ("replication_num" = "1");

-- Load data (e.g. Stream Load or INSERT).
```

2. **Run aggregate query** and measure:

```sql
SELECT SUM(value_col), COUNT(*), AVG(value_col) FROM olap_bench;
```

---

## 4. Snowflake

Snowflake is a cloud data warehouse.

1. **Create table and load** (e.g. from stage or INSERT):

```sql
CREATE OR REPLACE TABLE olap_bench (
  id BIGINT,
  key_col BIGINT,
  value_col BIGINT
);

-- Load 1M rows (e.g. COPY INTO from stage, or INSERT in batches).
```

2. **Run aggregate** and note latency (Snowflake reports query time in the UI):

```sql
SELECT SUM(value_col), COUNT(*), AVG(value_col) FROM olap_bench;
```

---

## 5. Databricks (Spark SQL)

Databricks uses Spark SQL over Delta Lake or Parquet.

1. **Create table and load** (PySpark or SQL):

```python
# Example: create Delta table and insert 1M rows
spark.sql("""
  CREATE TABLE olap_bench (id BIGINT, key_col BIGINT, value_col BIGINT)
  USING DELTA
""")
# Generate and insert 1M rows in batches
```

2. **Run aggregate** and measure end-to-end time:

```sql
SELECT SUM(value_col), COUNT(*), AVG(value_col) FROM olap_bench;
```

---

## Comparing Results

Use the **same**:

- **Row count** (e.g. 1M, 10M, or 100M)
- **Query**: `SELECT SUM(value_col), COUNT(*), AVG(value_col) FROM olap_bench`
- **Environment**: single node vs cluster; same machine class if possible

Then fill in the **comparison table** at the top of this doc (or your own spreadsheet). Use the same row count (e.g. 1M) and disable **result cache** where applicable (Snowflake: `ALTER SESSION SET USE_CACHED_RESULT = FALSE`; Databricks: `SET use_cached_result = false`).

**Notes:**

- RustafariDB runs **in-process** (no client/server), so latency is typically lower than over-the-wire systems. For a more level comparison, run other DBs on localhost with a single node.
- For **billions of rows**, ensure enough RAM or use disk-backed columnar storage; RustafariDB’s columnar store is currently in-memory with chunked batches (default 100k rows per chunk).

---

## Scaling to Billions

To approach **1B rows** with RustafariDB:

1. Use `--rows 1000000000` (or lower if memory-constrained).
2. Ensure the machine has sufficient RAM for columnar chunks (~8 bytes per value × columns × rows; e.g. 3 columns × 8 × 1e9 ≈ 24 GB for value data alone).
3. Run:  
   `cargo run -p rustafari-bench --release -- olap --rows 1000000000 --queries 10`

For production analytical workloads at that scale, consider persisting columnar data to disk (e.g. Parquet) and/or using the existing Parquet read path in `rustafari-lake`.
