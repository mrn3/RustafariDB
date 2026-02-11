//! Write-Ahead Log for durability and recovery.

use rustafari_core::{Result, RustafariError};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Type of WAL record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalRecord {
    /// Begin transaction.
    Begin { tx_id: u64 },
    /// Insert row.
    Insert {
        table_id: u64,
        row_id: u64,
        data: Vec<u8>,
    },
    /// Delete row.
    Delete { table_id: u64, row_id: u64 },
    /// Update row (full row).
    Update {
        table_id: u64,
        row_id: u64,
        data: Vec<u8>,
    },
    /// Commit transaction.
    Commit { tx_id: u64 },
    /// Abort transaction.
    Abort { tx_id: u64 },
}

/// Append-only WAL writer (simplified; production would use segment files and LSN).
pub struct WalWriter {
    writer: BufWriter<File>,
}

impl WalWriter {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let file = File::options()
            .create(true)
            .append(true)
            .open(path)
            .map_err(RustafariError::Io)?;
        Ok(Self {
            writer: BufWriter::new(file),
        })
    }

    pub fn append(&mut self, record: &WalRecord) -> Result<()> {
        let bytes = serde_json::to_vec(record).map_err(RustafariError::Serialization)?;
        let len = bytes.len() as u32;
        self.writer.write_all(&len.to_le_bytes()).map_err(RustafariError::Io)?;
        self.writer.write_all(&bytes).map_err(RustafariError::Io)?;
        self.writer.flush().map_err(RustafariError::Io)?;
        Ok(())
    }
}
