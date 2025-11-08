//! Section representation for tree-sitter parsed documents.
//!
//! A section represents a hierarchical division of a document, typically
//! corresponding to a heading in markdown. Sections track their position
//! in the document tree through parent/child relationships and maintain
//! precise byte and line coordinates for content extraction and modification.

#[derive(Clone)]
pub struct Section {
    pub title: String,
    pub level: usize,
    pub line_start: i64,
    pub line_end: i64,
    pub column_start: i64,
    pub column_end: i64,
    pub byte_start: usize,
    pub byte_end: usize,
    pub file_path: String,
    pub parent_index: Option<usize>,
    pub children_indices: Vec<usize>,
}
