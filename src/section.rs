//! Section representation for tree-sitter parsed documents.
//!
//! A section represents a hierarchical division of a document, typically
//! corresponding to a heading in markdown. Sections track their position
//! in the document tree through parent/child relationships and maintain
//! precise byte and line coordinates for content extraction and modification.

#[derive(Clone)]
/// Hierarchical document division with precise coordinates for extraction and modification.
pub struct Section {
    /// Section heading text without markup symbols.
    pub title: String,
    /// Nesting depth in the document hierarchy (1 for top-level).
    pub level: usize,
    /// First line of section content (after the heading).
    pub line_start: i64,
    /// Line where the next section begins or file ends.
    pub line_end: i64,
    /// Starting column of the section heading.
    pub column_start: i64,
    /// Ending column of the section heading.
    pub column_end: i64,
    /// Byte offset where section content begins.
    pub byte_start: usize,
    /// Byte offset where section content ends.
    pub byte_end: usize,
    /// Source file containing this section.
    pub file_path: String,
    /// Index of the containing section in the hierarchy.
    pub parent_index: Option<usize>,
    /// Indices of directly nested subsections.
    pub children_indices: Vec<usize>,
}
