//! Inverted index for full-text search.
//!
//! In-memory implementation: tokenize on whitespace, store term -> row_ids.
//! For production scale, integrate Tantivy or similar.

use rustafari_core::Result;
use rustafari_storage::RowId;
use std::collections::HashMap;
use std::path::Path;
use parking_lot::RwLock;

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split_whitespace()
        .map(|s| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Inverted index: term -> list of row IDs. Supports insert, delete, and search.
pub struct InvertedIndex {
    /// term -> vec of row_id (with duplicates for multiple occurrences per doc)
    index: RwLock<HashMap<String, Vec<RowId>>>,
    /// per-doc terms for deletion (row_id -> set of terms)
    doc_terms: RwLock<HashMap<RowId, Vec<String>>>,
}

impl InvertedIndex {
    pub fn new(_path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            index: RwLock::new(HashMap::new()),
            doc_terms: RwLock::new(HashMap::new()),
        })
    }

    pub fn insert(&self, row_id: RowId, text: &str) -> Result<()> {
        let terms = tokenize(text);
        for t in &terms {
            self.index
                .write()
                .entry(t.clone())
                .or_default()
                .push(row_id);
        }
        self.doc_terms.write().insert(row_id, terms);
        Ok(())
    }

    pub fn delete(&self, row_id: RowId) -> Result<()> {
        let terms = self.doc_terms.write().remove(&row_id);
        if let Some(terms) = terms {
            let mut idx = self.index.write();
            for t in &terms {
                if let Some(ids) = idx.get_mut(t) {
                    ids.retain(|&id| id != row_id);
                    if ids.is_empty() {
                        idx.remove(t);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn commit(&self) -> Result<()> {
        Ok(())
    }

    /// Full-text search: returns row IDs that contain all terms (AND) or any term (OR).
    /// Results are ordered by number of matching terms (desc). top_k limits the result.
    pub fn search(&self, query: &str, top_k: usize) -> Result<Vec<RowId>> {
        let terms = tokenize(query);
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let idx = self.index.read();
        let mut counts: HashMap<RowId, u32> = HashMap::new();
        for t in &terms {
            if let Some(ids) = idx.get(t) {
                for &id in ids {
                    *counts.entry(id).or_default() += 1;
                }
            }
        }
        let mut out: Vec<(RowId, u32)> = counts.into_iter().collect();
        out.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(out.into_iter().take(top_k).map(|(id, _)| id).collect())
    }
}
