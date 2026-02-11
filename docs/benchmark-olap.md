# OLAP Benchmark: RustafariDB vs Analytical Databases

This guide describes how to run **analytical (OLAP)** benchmarks so you can compare RustafariDB with **SingleStore**, **StarRocks**, **Snowflake**, and **Databricks** on aggregation workloads (SUM, COUNT, AVG over large tables).

## Workload

- **Schema**: One table with `id BIGINT`, `key_col BIGINT`, `value_col BIGINT`.
- **Queries**: `SELECT SUM(value_col), COUNT(*), AVG(value_col) FROM table`
- **Metrics**: Query latency (p50, p95, p99), queries per second, effective rows scanned per second.

Use the **same row count** and **same query** across systems for a fair comparison.

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

Then compare:

| System        | Rows   | Q/s  | Latency p50 (ms) | Latency p99 (ms) | Notes        |
|---------------|--------|------|------------------|------------------|--------------|
| RustafariDB   | 1M     | …    | …                | …                | In-process   |
| SingleStore   | 1M     | …    | …                | …                | Network hop  |
| StarRocks     | 1M     | …    | …                | …                | Network hop  |
| Snowflake     | 1M     | …    | …                | …                | Cloud        |
| Databricks    | 1M     | …    | …                | …                | Cluster      |

**Notes:**

- RustafariDB runs **in-process** (no client/server), so latency is lower than over-the-wire systems. For a more level comparison, run other DBs on localhost with a single node.
- For **billions of rows**, ensure enough RAM or use disk-backed columnar storage; RustafariDB’s columnar store is currently in-memory with chunked batches (default 100k rows per chunk).

---

## Scaling to Billions

To approach **1B rows** with RustafariDB:

1. Use `--rows 1000000000` (or lower if memory-constrained).
2. Ensure the machine has sufficient RAM for columnar chunks (~8 bytes per value × columns × rows; e.g. 3 columns × 8 × 1e9 ≈ 24 GB for value data alone).
3. Run:  
   `cargo run -p rustafari-bench --release -- olap --rows 1000000000 --queries 10`

For production analytical workloads at that scale, consider persisting columnar data to disk (e.g. Parquet) and/or using the existing Parquet read path in `rustafari-lake`.
