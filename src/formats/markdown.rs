//! Markdown format implementation using tree-sitter-md.
//!
//! This module provides tree-sitter queries for parsing markdown documents
//! and extracting section structure from ATX-style headings (# syntax).

use crate::formats::Format;

/// Tree-sitter queries for ATX-style markdown headings (# syntax).
pub struct MarkdownFormat;

impl Format for MarkdownFormat {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_md::LANGUAGE.into()
    }

    fn section_query(&self) -> &'static str {
        "(atx_heading) @heading"
    }

    fn title_query(&self) -> &'static str {
        "(atx_heading (atx_h1_marker)? (atx_h2_marker)? (atx_h3_marker)? (atx_h4_marker)? (atx_h5_marker)? (atx_h6_marker)? (inline) @title)"
    }
}
