# Benchmarking RustafariDB vs MySQL and PostgreSQL

This guide uses a **sysbench-style OLTP read-only** workload so you can compare RustafariDB with MySQL and PostgreSQL on a standard, widely used benchmark.

## Workload

The benchmark mirrors [sysbench](https://github.com/akopytov/sysbench) **oltp_read_only**:

- **Schema**: One table `sbtest` with columns `id`, `k`, `c`, `pad` (same logical schema as sysbench).
- **Operations per “transaction”**:
  - **Point SELECTs**: `SELECT c FROM sbtest WHERE id = ?` (default 10 per transaction).
  - **Range SELECTs**: `SELECT c FROM sbtest WHERE id >= ? AND id <= ? LIMIT range_size` (default 1 per transaction, range_size 100).
- **Metrics**: Queries per second (qps), transactions per second (tps), latency percentiles (p50, p95, p99).

Use the **same scale** (table size, duration) across databases so results are comparable.

---

## 1. RustafariDB

From the repo root:

```bash
cargo build -p rustafari-bench --release
cargo run -p rustafari-bench --release -- oltp --table-size 10000 --time 10
```

**Options** (match these with sysbench where possible):

| Option | Default | Description |
|--------|---------|-------------|
| `--table-size` | 10000 | Rows in `sbtest` (like sysbench `--table-size`) |
| `--time` | 10 | Run duration in seconds |
| `--point-selects` | 10 | Point SELECTs per transaction |
| `--simple-ranges` | 1 | Range SELECTs per transaction |
| `--range-size` | 100 | Range size for range SELECT |
| `--report-latency` | true | Print p50/p95/p99 latency |

**Example output:**

```
RustafariDB sysbench-style OLTP read-only benchmark
  table_size: 10000  point_selects: 10  simple_ranges: 1  range_size: 100

Loading 10000 rows...
  Loaded in 42.00ms (238095 rows/s)

Results:
  duration:     10.00s
  transactions: 345 (34.5 tps)
  queries:      3795 (379.5 qps)
  latency (ms): p50=28.50  p95=31.00  p99=35.00
```

---

## 2. MySQL (sysbench)

Install sysbench (e.g. `brew install sysbench` on macOS, or from [sysbench](https://github.com/akopytov/sysbench)).

**Prepare** (create DB and load data; use the same scale as RustafariDB):

```bash
mysql -u root -e "CREATE DATABASE IF NOT EXISTS sbtest;"
sysbench oltp_read_only \
  --db-driver=mysql \
  --mysql-host=127.0.0.1 \
  --mysql-port=3306 \
  --mysql-user=root \
  --mysql-password=YOUR_PASSWORD \
  --mysql-db=sbtest \
  --table-size=10000 \
  --tables=1 \
  prepare
```

**Run** (same duration and thread count for comparison):

```bash
sysbench oltp_read_only \
  --db-driver=mysql \
  --mysql-host=127.0.0.1 \
  --mysql-port=3306 \
  --mysql-user=root \
  --mysql-password=YOUR_PASSWORD \
  --mysql-db=sbtest \
  --table-size=10000 \
  --tables=1 \
  --time=10 \
  --threads=1 \
  run
```

Note the **queries** and **transactions** from the output and compare with RustafariDB.

---

## 3. PostgreSQL (sysbench)

**Prepare:**

```bash
createdb sbtest
sysbench oltp_read_only \
  --db-driver=pgsql \
  --pgsql-host=127.0.0.1 \
  --pgsql-port=5432 \
  --pgsql-user=YOUR_USER \
  --pgsql-db=sbtest \
  --table-size=10000 \
  --tables=1 \
  prepare
```

**Run:**

```bash
sysbench oltp_read_only \
  --db-driver=pgsql \
  --pgsql-host=127.0.0.1 \
  --pgsql-port=5432 \
  --pgsql-user=YOUR_USER \
  --pgsql-db=sbtest \
  --table-size=10000 \
  --tables=1 \
  --time=10 \
  --threads=1 \
  run
```

Again, compare **queries** and **transactions** (and latency if reported) with RustafariDB.

---

## Comparing results

Use the **same**:

- `--table-size` / `--table-size` (e.g. 10000)
- `--time` / `--time` (e.g. 10)
- Single thread for a like-for-like baseline (`--threads=1` for sysbench; RustafariDB benchmark is single-threaded)

Then compare:

- **Queries per second (qps)** and **transactions per second (tps)**.
- **Latency** (p50, p95, p99) where available.

RustafariDB’s benchmark runs **in-process** (no client/server), while MySQL and PostgreSQL are measured over the wire with sysbench. For a more level comparison you can run sysbench with a local connection and a single thread as above.

---

## Optional: run only a fixed number of events

RustafariDB benchmark:

```bash
cargo run -p rustafari-bench --release -- --table-size 10000 --time 0 --total-events 1000
```

This runs exactly 1000 transactions (and then stops; `--time 0` means “ignore duration”).
