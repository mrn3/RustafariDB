//! Vector index for similarity search.
//!
//! This implementation uses brute-force L2 search. Production deployments
//! can integrate HNSW (e.g. hnsw or hnsw_rs crates) for approximate nearest neighbor.

use rustafari_core::{Result, RustafariError};
use rustafari_storage::RowId;
use std::sync::RwLock;

fn l2_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Vector index: stores (row_id, vector) and supports k-NN search (brute-force).
/// For large-scale ANN, plug in an HNSW or other ANN implementation.
pub struct VectorIndex {
    dimension: usize,
    entries: RwLock<Vec<(RowId, Vec<f32>)>>,
}

impl VectorIndex {
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            entries: RwLock::new(Vec::new()),
        }
    }

    pub fn insert(&self, row_id: RowId, vector: Vec<f32>) -> Result<()> {
        if vector.len() != self.dimension {
            return Err(RustafariError::Index(format!(
                "vector dimension {} != index dimension {}",
                vector.len(),
                self.dimension
            )));
        }
        self.entries
            .write()
            .map_err(|_| RustafariError::Index("lock poisoned".into()))?
            .push((row_id, vector));
        Ok(())
    }

    /// k-NN search by L2 distance. Returns (row_id, distance) for top_k.
    pub fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<(RowId, f32)>> {
        if query.len() != self.dimension {
            return Err(RustafariError::Index(format!(
                "query dimension {} != index dimension {}",
                query.len(),
                self.dimension
            )));
        }
        let g = self.entries
            .read()
            .map_err(|_| RustafariError::Index("lock poisoned".into()))?;
        let mut with_dist: Vec<(RowId, f32)> = g
            .iter()
            .map(|(rid, v)| (*rid, l2_distance(query, v)))
            .collect();
        with_dist.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        with_dist.truncate(top_k);
        Ok(with_dist)
    }
}
