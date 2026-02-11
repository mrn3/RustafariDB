# RustafariDB

A **unified database** built in Rust that combines:

- **Transactional (OLTP)** – ACID transactions, row storage, B-tree indexes
- **Analytical (OLAP)** – Columnar storage, real-time analytics (StarRocks/SingleStore style)
- **Search** – Full-text with inverted indexes
- **Vector search** – k-NN over embeddings
- **SQL** – PostgreSQL-style dialect
- **MongoDB-style API** – Document collections with find/filter/update DSL
- **Data lake** – Parquet read/write and Apache Iceberg integration path

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     rustafari-server (CLI / gRPC)               │
├─────────────────────────────────────────────────────────────────┤
│  rustafari-sql (parser)  │  rustafari-document (MongoDB API)   │
├──────────────────────────┼──────────────────────────────────────┤
│       rustafari-executor (SQL + columnar execution)             │
├──────────────────────────┼──────────────────────────────────────┤
│  rustafari-storage       │  rustafari-index                     │
│  (row + columnar + WAL)  │  (B-tree, inverted, vector)          │
├──────────────────────────┼──────────────────────────────────────┤
│  rustafari-core (types, catalog, transactions)                  │
├──────────────────────────┴──────────────────────────────────────┤
│  rustafari-lake (Parquet, Iceberg)                              │
└─────────────────────────────────────────────────────────────────┘
```

## Crates

| Crate | Purpose |
|-------|---------|
| **rustafari-core** | Value, Row, Schema, Catalog, TransactionId, errors |
| **rustafari-storage** | Page, WAL, TableStore (row), ColumnarStore (chunks) |
| **rustafari-index** | B-tree, inverted (full-text), vector (k-NN) indexes |
| **rustafari-sql** | SQL parser (sqlparser), logical plan (Scan/Filter/Project/Limit/Insert/CreateTable) |
| **rustafari-document** | MongoDB-style filter DSL, DocumentCollection (find/insert/delete) |
| **rustafari-executor** | Execute plans, SessionState, columnar → Arrow RecordBatch |
| **rustafari-lake** | Read/write Parquet; Iceberg table reference (integration path) |
| **rustafari-server** | CLI shell, single-query, and serve entrypoint |

## Quick Start

```bash
cargo build --release
```

**Interactive SQL shell:**

```bash
cargo run -p rustafari-server -- shell
```

Then:

```sql
CREATE TABLE users (id BIGINT, name TEXT, age BIGINT);
INSERT INTO users VALUES (1, 'alice', 30), (2, 'bob', 25);
SELECT name, age FROM users WHERE age >= 25 LIMIT 10;
```

**Single query:**

```bash
cargo run -p rustafari-server -- query --sql "SELECT 1"
```

## Features

### OLTP (Transactional)

- Row storage with stable `RowId`
- WAL for durability (append-only log)
- B-tree indexes for point and range lookups
- Transaction context (ID, snapshot, isolation level) for future MVCC

### OLAP (Analytical)

- Columnar chunks (`ColumnarChunk`, `ColumnChunk`) for vectorized execution
- Conversion to Arrow `RecordBatch` for analytics
- Parquet read/write for data lake interchange

### Search

- **Inverted index** – tokenize text, store term → row IDs; support AND/OR and phrase-style search
- **Vector index** – L2 k-NN over `Vec<f32>` (brute-force by default; HNSW can be plugged in)

### SQL

- Parser: **sqlparser** (ANSI SQL–style)
- Dialect: PostgreSQL-compatible
- Supported: `SELECT`, `INSERT ... VALUES`, `CREATE TABLE`, `WHERE`, `LIMIT`/`OFFSET`, projections

### MongoDB-style API

- **Collections** backed by a table (schema + row store)
- **Filter DSL** – `{ "field": value }`, `{ "field": { "$gte": 21 } }`, `$and`/`$or`/`$not`
- **find** / **find_one** / **insert_one** / **delete_many**

### Data Lake

- **Parquet** – read/write Arrow RecordBatches to/from Parquet files
- **Iceberg** – type and docs for integrating the official `iceberg` crate (Arrow 57+); use for transactional tables over object storage

## Indexes

- **B-tree** – `IndexKey` from `Value` (int, float, string, timestamp, date); point and range scan
- **Inverted** – in-memory term → row IDs; optional Tantivy for production full-text
- **Vector** – dimension fixed; `insert(row_id, vector)`, `search(query, top_k)`; replace with HNSW for scale

## Benchmarking

- **OLTP** (sysbench-style): compare with MySQL and PostgreSQL  
  `cargo run -p rustafari-bench --release -- oltp --table-size 10000 --time 10`  
  See **[docs/benchmark.md](docs/benchmark.md)**.

- **OLAP** (columnar SUM/COUNT/AVG): compare with SingleStore, StarRocks, Snowflake, Databricks  
  `cargo run -p rustafari-bench --release -- olap --rows 1000000 --queries 100`  
  See **[docs/benchmark-olap.md](docs/benchmark-olap.md)**.

## Configuration

- Default namespace: `public`
- Storage: in-memory (TableStore, ColumnarStore); WAL and page layer are in place for persistence
- Server: `--addr 127.0.0.1:50051` for future gRPC

## License

MIT or Apache-2.0, at your option.
