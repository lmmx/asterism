//! Format trait and implementations for different document types.
//!
//! This module defines the `Format` trait which abstracts over different
//! document formats (markdown, org-mode, restructuredtext, etc.) by providing
//! tree-sitter queries specific to each format.

pub mod markdown;

pub trait Format {
    fn language(&self) -> tree_sitter::Language;
    fn section_query(&self) -> &str;
    fn title_query(&self) -> &str;
}
