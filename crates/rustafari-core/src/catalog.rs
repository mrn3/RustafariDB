//! Catalog: namespaces, tables, metadata.

use crate::schema::Schema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Serializable snapshot of the catalog for persistence.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CatalogSnapshot {
    /// Namespace name -> id.
    pub namespaces: Vec<(String, u64)>,
    /// All table metadata.
    pub tables: Vec<TableMeta>,
}

/// Namespace (schema/database) identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NamespaceId(pub u64);

/// Table identifier (unique across namespaces).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TableId(pub u64);

/// Table metadata in catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMeta {
    pub id: TableId,
    pub namespace_id: NamespaceId,
    pub name: String,
    pub schema: Schema,
    /// Physical storage: "row" (OLTP) or "columnar" (OLAP) or "hybrid".
    pub storage_format: String,
}

impl TableMeta {
    pub fn new(id: TableId, namespace_id: NamespaceId, name: String, schema: Schema) -> Self {
        Self {
            id,
            namespace_id,
            name,
            schema,
            storage_format: "row".to_string(),
        }
    }
}

/// In-memory catalog (can be backed by storage later).
#[derive(Debug, Default)]
pub struct Catalog {
    next_namespace_id: AtomicU64,
    next_table_id: AtomicU64,
    namespaces: HashMap<String, NamespaceId>,
    tables_by_name: HashMap<(NamespaceId, String), TableMeta>,
    tables_by_id: HashMap<TableId, TableMeta>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_namespace(&mut self, name: impl Into<String>) -> NamespaceId {
        let name = name.into();
        if let Some(&id) = self.namespaces.get(&name) {
            return id;
        }
        let id = NamespaceId(self.next_namespace_id.fetch_add(1, Ordering::SeqCst));
        self.namespaces.insert(name, id);
        id
    }

    pub fn namespace_id(&self, name: &str) -> Option<NamespaceId> {
        self.namespaces.get(name).copied()
    }

    /// List all namespace (database) names.
    pub fn list_namespaces(&self) -> Vec<String> {
        let mut names: Vec<String> = self.namespaces.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn create_table(
        &mut self,
        namespace: impl AsRef<str>,
        name: impl Into<String>,
        schema: Schema,
        storage_format: Option<&str>,
    ) -> TableMeta {
        let ns_name = namespace.as_ref();
        let namespace_id = self.namespace_id(ns_name).unwrap_or_else(|| {
            let id = NamespaceId(self.next_namespace_id.fetch_add(1, Ordering::SeqCst));
            self.namespaces.insert(ns_name.to_string(), id);
            id
        });
        let name = name.into();
        let id = TableId(self.next_table_id.fetch_add(1, Ordering::SeqCst));
        let mut meta = TableMeta::new(id, namespace_id, name.clone(), schema);
        if let Some(s) = storage_format {
            meta.storage_format = s.to_string();
        }
        self.tables_by_name
            .insert((namespace_id, name), meta.clone());
        self.tables_by_id.insert(id, meta.clone());
        meta
    }

    pub fn get_table(&self, namespace: &str, name: &str) -> Option<&TableMeta> {
        let namespace_id = self.namespace_id(namespace)?;
        self.tables_by_name.get(&(namespace_id, name.to_string()))
    }

    pub fn get_table_by_id(&self, id: TableId) -> Option<&TableMeta> {
        self.tables_by_id.get(&id)
    }

    pub fn list_tables(&self, namespace: &str) -> Vec<&TableMeta> {
        let Some(ns_id) = self.namespace_id(namespace) else {
            return Vec::new();
        };
        self.tables_by_name
            .iter()
            .filter(|((ns, _), _)| *ns == ns_id)
            .map(|(_, m)| m)
            .collect()
    }

    /// Remove a table from the catalog. Returns the table metadata if it existed.
    pub fn drop_table(&mut self, namespace: &str, name: &str) -> Option<TableMeta> {
        let namespace_id = self.namespace_id(namespace)?;
        let meta = self.tables_by_name.remove(&(namespace_id, name.to_string()))?;
        self.tables_by_id.remove(&meta.id);
        Some(meta)
    }

    /// Build a serializable snapshot for persistence.
    pub fn to_snapshot(&self) -> CatalogSnapshot {
        let namespaces: Vec<(String, u64)> = self
            .namespaces
            .iter()
            .map(|(name, id)| (name.clone(), id.0))
            .collect();
        let tables: Vec<TableMeta> = self.tables_by_id.values().cloned().collect();
        CatalogSnapshot {
            namespaces,
            tables,
        }
    }

    /// Load from a snapshot (e.g. on startup). Rebuilds name_to_index on each table's schema.
    pub fn load_snapshot(&mut self, snap: CatalogSnapshot) {
        self.namespaces.clear();
        self.tables_by_name.clear();
        self.tables_by_id.clear();
        let mut max_ns = 0u64;
        let mut max_table = 0u64;
        for (name, id) in snap.namespaces {
            max_ns = max_ns.max(id);
            self.namespaces.insert(name, NamespaceId(id));
        }
        for mut meta in snap.tables {
            max_table = max_table.max(meta.id.0);
            meta.schema.rebuild_name_to_index();
            self.tables_by_name
                .insert((meta.namespace_id, meta.name.clone()), meta.clone());
            self.tables_by_id.insert(meta.id, meta);
        }
        self.next_namespace_id.store(max_ns + 1, Ordering::SeqCst);
        self.next_table_id.store(max_table + 1, Ordering::SeqCst);
    }
}
