//! Difftastic format implementation for displaying structural diffs.
//!
//! This module provides support for parsing difftastic JSON output and
//! converting it into sections that can be navigated and edited in asterism.

use crate::formats::Format;
use crate::section::{ChunkType, Section};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Write;
use std::path::Path;
use std::{fs, io};

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

fn create_chunk_section(
    file_path: &str,
    title: String,
    line_num: i64,
    column_start: i64,
    column_end: i64,
    chunk_type: ChunkType,
    lhs_text: Option<String>,
    rhs_text: Option<String>,
) -> Section {
    Section {
        title,
        level: 2,
        line_start: line_num,
        line_end: line_num + 1,
        column_start,
        column_end,
        byte_start: 0,
        byte_end: 0,
        file_path: file_path.to_string(),
        parent_index: None,
        children_indices: Vec::new(),
        doc_comment: None,
        chunk_type: Some(chunk_type),
        lhs_content: lhs_text,
        rhs_content: rhs_text,
    }
}

/// Parse difftastic JSON output into sections
///
/// Files become non-navigable containers, hunks become navigable sections.
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
        // Skip unchanged files
        if file.status == "unchanged" {
            continue;
        }

        let file_path = &file.path;

        // Create hunk sections directly (no file section)
        if let Some(chunks) = &file.chunks {
            for (hunk_idx, chunk) in chunks.iter().enumerate() {
                let hunk_title = format_hunk_title(chunk, hunk_idx);
                let hunk_content = format_hunk_content(chunk);

                let hunk_start_line = global_line;
                let hunk_end_line =
                    global_line + i64::try_from(hunk_content.lines().count()).unwrap_or(0);

                // Create section for this hunk
                sections.push(Section {
                    title: hunk_title,
                    level: 1, // All hunks are top-level sections
                    line_start: hunk_start_line,
                    line_end: hunk_end_line,
                    column_start: 0,
                    column_end: 0,
                    byte_start: 0,
                    byte_end: 0,
                    file_path: file_path.clone(),
                    parent_index: None,
                    children_indices: Vec::new(),
                    doc_comment: Some(vec![hunk_content]),
                    chunk_type: None,
                    lhs_content: None,
                    rhs_content: None,
                });

                global_line = hunk_end_line + 1;
            }
        } else if file.status == "created" || file.status == "deleted" {
            // For files with no chunks (created/deleted without detailed hunks),
            // create a single hunk showing the status
            let hunk_title = format!("File {} (no detailed diff available)", file.status);
            let hunk_content = format!("File was {}", file.status);

            sections.push(Section {
                title: hunk_title,
                level: 1,
                line_start: global_line,
                line_end: global_line + 1,
                column_start: 0,
                column_end: 0,
                byte_start: 0,
                byte_end: 0,
                file_path: file_path.clone(),
                parent_index: None,
                children_indices: Vec::new(),
                doc_comment: Some(vec![hunk_content]),
                chunk_type: None,
                lhs_content: None,
                rhs_content: None,
            });

            global_line += 2;
        }
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

fn extract_chunk_text(side: &Value) -> Option<String> {
    side.get("changes")
        .and_then(|c| c.as_array())
        .map(|changes| {
            changes
                .iter()
                .filter_map(|change| change.get("content").and_then(|c| c.as_str()))
                .collect::<String>()
        })
}

fn extract_column_range(side: &Value) -> (i64, i64) {
    let changes = side.get("changes").and_then(|c| c.as_array());

    let start = changes
        .and_then(|arr| arr.first())
        .and_then(|first| first.get("start"))
        .and_then(|s| s.as_i64())
        .unwrap_or(0);

    let end = changes
        .and_then(|arr| arr.last())
        .and_then(|last| last.get("end"))
        .and_then(|e| e.as_i64())
        .unwrap_or(0);

    (start, end)
}

/// Extract the difftastic hunks as sections (same as sections in a markdown etc)
pub fn extract_difftastic_sections(json_path: &Path) -> io::Result<Vec<Section>> {
    let content = fs::read_to_string(json_path)?;
    let lines: Vec<Value> = content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    let mut sections = Vec::new();

    for value in lines {
        let file_path = value
            .get("path")
            .and_then(|p| p.as_str())
            .unwrap_or("unknown");

        if let Some(chunks) = value.get("chunks").and_then(|c| c.as_array()) {
            for chunk_array in chunks {
                if let Some(chunk_list) = chunk_array.as_array() {
                    for chunk in chunk_list {
                        let lhs = chunk.get("lhs");
                        let rhs = chunk.get("rhs");

                        let chunk_type = match (lhs, rhs) {
                            (Some(_), None) => ChunkType::Deleted,
                            (None, Some(_)) => ChunkType::Added,
                            (Some(l), Some(r)) if l != r => ChunkType::Modified,
                            (Some(_), Some(_)) => ChunkType::Unchanged,
                            _ => continue,
                        };

                        let line_num = lhs
                            .or(rhs)
                            .and_then(|v| v.get("line_number"))
                            .and_then(|n| n.as_i64())
                            .unwrap_or(0);

                        let (column_start, column_end) =
                            lhs.or(rhs).map(extract_column_range).unwrap_or((0, 0));

                        let title = format!("Chunk @@ {}:{} @@", file_path, line_num);
                        let lhs_text = lhs.and_then(extract_chunk_text);
                        let rhs_text = rhs.and_then(extract_chunk_text);

                        sections.push(create_chunk_section(
                            file_path,
                            title,
                            line_num,
                            column_start,
                            column_end,
                            chunk_type,
                            lhs_text,
                            rhs_text,
                        ));
                    }
                }
            }
        }
    }

    Ok(sections)
}

#[cfg(test)]
#[path = "../tests/difftastic.rs"]
mod tests;
