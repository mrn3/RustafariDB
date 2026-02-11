//! # RustafariDB Lake
//!
//! Data lake integration: read/write Parquet, and interface for Apache Iceberg.
//! Enables querying data lakes with real-time analytics (StarRocks/SingleStore style)
//! and data lakehouse patterns (Databricks/Snowflake style).

pub mod parquet_io;
pub mod iceberg;

pub use parquet_io::{read_parquet, write_parquet};
pub use iceberg::IcebergTableRef;
