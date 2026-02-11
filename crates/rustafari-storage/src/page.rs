//! Page-based storage (8KB pages for B-tree and row storage).

use rustafari_core::RustafariError;
use serde::{Deserialize, Serialize};
use std::ops::Range;

pub const PAGE_SIZE: usize = 8192;

/// Unique page identifier (file id + page number).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageId {
    pub file_id: u32,
    pub page_no: u64,
}

impl PageId {
    pub fn new(file_id: u32, page_no: u64) -> Self {
        Self { file_id, page_no }
    }
}

/// A single page (fixed size buffer).
#[derive(Clone)]
pub struct Page {
    pub id: PageId,
    pub data: [u8; PAGE_SIZE],
    pub dirty: bool,
}

impl Page {
    pub fn new(id: PageId) -> Self {
        Self {
            id,
            data: [0u8; PAGE_SIZE],
            dirty: false,
        }
    }

    pub fn read(&self, range: Range<usize>) -> Result<&[u8], RustafariError> {
        if range.end > PAGE_SIZE {
            return Err(RustafariError::Storage("read out of page bounds".into()));
        }
        Ok(&self.data[range])
    }

    pub fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), RustafariError> {
        if offset + data.len() > PAGE_SIZE {
            return Err(RustafariError::Storage("write out of page bounds".into()));
        }
        self.data[offset..offset + data.len()].copy_from_slice(data);
        self.dirty = true;
        Ok(())
    }
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("id", &self.id)
            .field("dirty", &self.dirty)
            .field("data_len", &self.data.len())
            .finish()
    }
}
