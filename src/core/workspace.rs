//! A workspace is an ordered collection of log inputs and their parsed sources.
//! Items are retained separately so removal drops exactly the sources that
//! input produced (unaffected by source-name collisions across inputs).

use crate::models::LogSource;

#[derive(Debug, Clone)]
pub struct WorkspaceItem {
    /// Display path of the input as the user added it (file, archive, or folder).
    pub path: String,
    /// Parsed sources produced by ingesting `path`.
    pub sources: Vec<LogSource>,
}

#[derive(Debug, Clone, Default)]
pub struct Workspace {
    items: Vec<WorkspaceItem>,
}

impl Workspace {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// True if `path` is already present (exact trimmed-string match).
    pub fn contains(&self, path: &str) -> bool {
        self.items.iter().any(|i| i.path == path)
    }

    /// Append an item. No dedup here — callers decide.
    pub fn add(&mut self, path: String, sources: Vec<LogSource>) {
        self.items.push(WorkspaceItem { path, sources });
    }

    /// Remove by index; returns the removed item or None if out of range.
    pub fn remove(&mut self, index: usize) -> Option<WorkspaceItem> {
        if index < self.items.len() {
            Some(self.items.remove(index))
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn items(&self) -> &[WorkspaceItem] {
        &self.items
    }

    /// Flattened view of every item's sources, in item order.
    pub fn merged_sources(&self) -> Vec<LogSource> {
        self.items
            .iter()
            .flat_map(|i| i.sources.clone())
            .collect()
    }
}
