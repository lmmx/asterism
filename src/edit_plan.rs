//! The edit plan manages document modifications using textum patches.
//!
//! This module defines the transformation that work in the TUI manifests as actual edits on disk.
//! asterism uses textum for generic line-based patching that works with any text format.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use textum::{Boundary, BoundaryMode, Patch, PatchSet, Snippet, Target};

#[derive(Serialize, Deserialize, Clone)]
pub struct EditPlan {
    pub edits: Vec<Edit>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Edit {
    pub file_name: String,
    pub line_start: i64,
    pub line_end: i64,
    pub column_start: i64,
    pub column_end: i64,
    pub doc_comment: String,
    pub item_name: String,
}

impl EditPlan {
    /// Apply all edits in the plan using textum patches.
    ///
    /// Groups edits by file and uses textum's `PatchSet` to apply all changes
    /// atomically per file. Each edit targets a line range and replaces the
    /// content between those lines with the new section content.
    ///
    /// # Errors
    ///
    /// Returns an error if file operations or patching fails.
    pub fn apply(&mut self) -> io::Result<()> {
        let mut file_groups: HashMap<String, Vec<&Edit>> = HashMap::new();

        for edit in &self.edits {
            file_groups
                .entry(edit.file_name.clone())
                .or_default()
                .push(edit);
        }

        for (file_name, edits) in file_groups {
            let mut patchset = PatchSet::new();

            for edit in edits {
                let start = Boundary::new(
                    Target::Line(edit.line_start.try_into().unwrap()),
                    BoundaryMode::Include,
                );
                let end = Boundary::new(
                    Target::Line(edit.line_end.try_into().unwrap()),
                    BoundaryMode::Exclude,
                );
                let snippet = Snippet::Between { start, end };

                let replacement = format!("\n{}\n", edit.doc_comment.trim());

                let patch = Patch {
                    file: file_name.clone(),
                    snippet,
                    replacement,
                };

                patchset.add(patch);
            }

            let results = patchset
                .apply_to_files()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

            if let Some(new_content) = results.get(&file_name) {
                std::fs::write(&file_name, new_content)?;
            }
        }

        Ok(())
    }
}
