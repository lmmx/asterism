//! Difftastic format implementation for displaying structural diffs.
//!
//! This module provides support for parsing difftastic JSON output and
//! converting it into sections that can be navigated and edited in asterism.

use crate::formats::Format;
use crate::section::Section;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::io;

/// Represents a file in difftastic output
#[derive(Debug, Serialize, Deserialize)]
pub struct DifftFile {
    /// Programming language identified by difftastic for syntax highlighting.
    pub language: String,
    /// File path relative to the comparison root.
    pub path: String,
    /// Grouped diff hunks, each containing lines that changed together.
    #[serde(default)]
    pub chunks: Option<Vec<Vec<DifftLine>>>,
    /// Change classification: "unchanged", "changed", "created", or "deleted".
    pub status: String,
}

/// Represents a line in a diff chunk
#[derive(Debug, Serialize, Deserialize)]
pub struct DifftLine {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Left-hand (original) side of the comparison, absent for pure additions.
    pub lhs: Option<DifftSide>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Right-hand (modified) side of the comparison, absent for pure deletions.
    pub rhs: Option<DifftSide>,
}

/// Represents one side (left or right) of a diff line
#[derive(Debug, Serialize, Deserialize)]
pub struct DifftSide {
    /// Original line position in the source file (1-indexed).
    pub line_number: u32,
    /// Structural changes within this line, ordered by column position.
    pub changes: Vec<DifftChange>,
}

/// Represents a change within a line
#[derive(Debug, Serialize, Deserialize)]
pub struct DifftChange {
    /// Column offset where this change begins (0-indexed).
    pub start: u32,
    /// Column offset where this change ends (exclusive).
    pub end: u32,
    /// Text content of this change segment.
    pub content: String,
    /// Syntax category for rendering: "delimiter", "string", "keyword", "comment", "type", "normal"
    /// or "`tree_sitter_error`".
    pub highlight: String,
}

/// Difftastic format handler
pub struct DifftasticFormat;

impl Format for DifftasticFormat {
    fn file_extension(&self) -> &'static str {
        "diff"
    }

    fn language(&self) -> tree_sitter::Language {
        // Difftastic doesn't use tree-sitter parsing
        tree_sitter_md::LANGUAGE.into()
    }

    fn section_query(&self) -> &'static str {
        ""
    }

    fn title_query(&self) -> &'static str {
        ""
    }

    fn format_section_display(&self, level: usize, title: &str) -> Line<'static> {
        let color = if level == 0 {
            Color::Cyan // Files
        } else {
            Color::Yellow // Hunks
        };

        let spans = vec![
            Span::styled("● ", Style::default().fg(color)),
            Span::raw(title.to_string()),
        ];

        Line::from(spans)
    }
}

/// Parse difftastic JSON output into sections
///
/// Handles both array format and newline-delimited JSON (NDJSON) format.
///
/// # Errors
///
/// Returns an error if JSON parsing fails or if the format is invalid.
pub fn parse_difftastic_json(json_str: &str) -> io::Result<Vec<Section>> {
    let files: Vec<DifftFile> = if let Ok(files) = serde_json::from_str::<Vec<DifftFile>>(json_str)
    {
        // Array format: [{file1}, {file2}]
        files
    } else if json_str.trim().starts_with('[') {
        // Failed to parse as array, invalid format
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid JSON array format",
        ));
    } else {
        // Try parsing as newline-delimited JSON (NDJSON/JSON Lines)
        // This is what git outputs when there are multiple files
        json_str
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str::<DifftFile>(line).map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Failed to parse JSON line: {e}"),
                    )
                })
            })
            .collect::<Result<Vec<DifftFile>, io::Error>>()?
    };

    let mut sections = Vec::new();
    let mut global_line = 0i64;

    for file in &files {
        // Skip only unchanged files (show created, deleted, and changed)
        if file.status == "unchanged" {
            continue;
        }

        // Create file-level section
        let file_title = format!("{} ({})", file.path, file.status);
        let file_start_line = global_line;

        sections.push(Section {
            title: file_title,
            level: 1,
            line_start: file_start_line,
            line_end: file_start_line + 1,
            column_start: 0,
            column_end: 0,
            byte_start: 0,
            byte_end: 0,
            file_path: file.path.clone(),
            parent_index: None,
            children_indices: Vec::new(),
            doc_comment: None,
        });

        let file_section_idx = sections.len() - 1;
        global_line += 1;

        // Create hunk sections
        if let Some(chunks) = &file.chunks {
            for (hunk_idx, chunk) in chunks.iter().enumerate() {
                let hunk_title = format_hunk_title(chunk, hunk_idx);
                let hunk_content = format_hunk_content(chunk);

                let hunk_start_line = global_line;
                let hunk_end_line =
                    global_line + i64::try_from(hunk_content.lines().count()).unwrap_or(0);

                sections.push(Section {
                    title: hunk_title,
                    level: 2,
                    line_start: hunk_start_line,
                    line_end: hunk_end_line,
                    column_start: 0,
                    column_end: 0,
                    byte_start: 0,
                    byte_end: 0,
                    file_path: file.path.clone(),
                    parent_index: Some(file_section_idx),
                    children_indices: Vec::new(),
                    doc_comment: Some(vec![hunk_content]),
                });

                let new_section_idx = sections.len() - 1;
                sections[file_section_idx]
                    .children_indices
                    .push(new_section_idx);
                global_line = hunk_end_line + 1;
            }
        } else if file.status == "created" || file.status == "deleted" {
            // For files with no chunks (created/deleted files without detailed hunks),
            // create a placeholder hunk showing the status
            let hunk_title = format!("File {} (no detailed diff available)", file.status);

            sections.push(Section {
                title: hunk_title,
                level: 2,
                line_start: global_line,
                line_end: global_line + 1,
                column_start: 0,
                column_end: 0,
                byte_start: 0,
                byte_end: 0,
                file_path: file.path.clone(),
                parent_index: Some(file_section_idx),
                children_indices: Vec::new(),
                doc_comment: Some(vec![format!("File was {}", file.status)]),
            });

            let new_section_idx = sections.len() - 1;
            sections[file_section_idx]
                .children_indices
                .push(new_section_idx);
            global_line += 2;
        }

        sections[file_section_idx].line_end = global_line;
    }

    Ok(sections)
}

/// Format a hunk title showing line number range
fn format_hunk_title(chunk: &[DifftLine], hunk_idx: usize) -> String {
    let mut lhs_lines = Vec::new();
    let mut rhs_lines = Vec::new();

    for line in chunk {
        if let Some(ref lhs) = line.lhs {
            lhs_lines.push(lhs.line_number);
        }
        if let Some(ref rhs) = line.rhs {
            rhs_lines.push(rhs.line_number);
        }
    }

    match (
        lhs_lines.first(),
        lhs_lines.last(),
        rhs_lines.first(),
        rhs_lines.last(),
    ) {
        (Some(lhs_start), Some(lhs_end), Some(rhs_start), Some(rhs_end)) => {
            format!(
                "Hunk {} (@@ -{},{} +{},{} @@)",
                hunk_idx + 1,
                lhs_start,
                lhs_end - lhs_start + 1,
                rhs_start,
                rhs_end - rhs_start + 1
            )
        }
        (Some(lhs_start), Some(lhs_end), None, None) => {
            format!(
                "Hunk {} (deletion @@ -{},{} @@)",
                hunk_idx + 1,
                lhs_start,
                lhs_end - lhs_start + 1
            )
        }
        (None, None, Some(rhs_start), Some(rhs_end)) => {
            format!(
                "Hunk {} (addition @@ +{},{} @@)",
                hunk_idx + 1,
                rhs_start,
                rhs_end - rhs_start + 1
            )
        }
        _ => format!("Hunk {}", hunk_idx + 1),
    }
}

/// Format hunk content for display
fn format_hunk_content(chunk: &[DifftLine]) -> String {
    let mut output = String::new();

    for line in chunk {
        match (&line.lhs, &line.rhs) {
            (Some(lhs), Some(rhs)) => {
                // Modified line - show both sides
                write!(output, "-{}: ", lhs.line_number).unwrap();
                for change in &lhs.changes {
                    output.push_str(&change.content);
                }
                output.push('\n');

                write!(output, "+{}: ", rhs.line_number).unwrap();
                for change in &rhs.changes {
                    output.push_str(&change.content);
                }
                output.push('\n');
            }
            (Some(lhs), None) => {
                // Deleted line
                write!(output, "-{}: ", lhs.line_number).unwrap();
                for change in &lhs.changes {
                    output.push_str(&change.content);
                }
                output.push('\n');
            }
            (None, Some(rhs)) => {
                // Added line
                write!(output, "+{}: ", rhs.line_number).unwrap();
                for change in &rhs.changes {
                    output.push_str(&change.content);
                }
                output.push('\n');
            }
            (None, None) => {
                // Context line (shouldn't happen in difftastic output)
                output.push_str(" \n");
            }
        }
    }

    output
}

#[cfg(test)]
#[path = "../tests/difftastic.rs"]
mod tests;
