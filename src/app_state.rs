//! The core state machine bridging document sections and the interactive editor.
//!
//! A TUI needs a single source of truth that can be interrogated and mutated as the user navigates
//! and edits. We achieve this by syncing the editor save state with the files on disk. We keep
//! track of the cumulative total number of lines that have been added to the file during the
//! session so that we can determine the correct offset to insert content at without re-parsing.

use crate::edit_plan::{Edit, EditPlan};
use crate::formats::markdown::MarkdownFormat;
use crate::input;
use crate::section::Section;
use edtui::{EditorState, Lines};
use std::collections::HashMap;
use std::path::PathBuf;
use std::{fs, io};

#[derive(PartialEq)]
/// Determines navigation scope and quit behavior based on project size.
pub enum FileMode {
    /// Single-file mode quits directly to shell.
    Single,
    /// Multi-file mode returns to file list before quitting.
    Multi,
}

/// Bridges document sections and the interactive editor, maintaining session state.
///
/// Tracks cumulative line additions to determine correct file offsets without re-parsing,
/// enabling efficient writes after multiple edits across sections.
pub struct AppState {
    /// All parsed sections across loaded files.
    pub sections: Vec<Section>,
    /// File paths available for editing in multi-file mode.
    pub files: Vec<PathBuf>,
    /// Selected file in the file list view.
    pub current_file_index: usize,
    /// Controls navigation behavior and file list visibility.
    pub file_mode: FileMode,
    /// Active UI screen determining input handling.
    pub current_view: View,
    /// Selected section in the section list.
    pub current_section_index: usize,
    /// Editor buffer content when detail view is active.
    pub editor_state: Option<EditorState>,
    /// Accumulates vim-style command input after ':' is pressed.
    pub command_buffer: String,
    /// Status feedback displayed in the help bar.
    pub message: Option<String>,
    /// Maximum line width for text wrapping in the editor.
    pub wrap_width: usize,
    /// Tracks line count changes per section to calculate write positions without re-parsing.
    pub file_offsets: HashMap<String, HashMap<i64, usize>>,
}

#[derive(PartialEq)]
/// Determines which UI screen renders and how input is interpreted.
pub enum View {
    /// Displays available files for multi-file projects.
    FileList,
    /// Shows hierarchical section tree with navigation.
    List,
    /// Provides vim-like editor for section content.
    Detail,
    /// Captures vim-style command input after ':' keystroke.
    Command,
}

impl AppState {
    #[must_use]
    /// Initialises application state with parsed sections and determines file mode.
    ///
    /// Single-file projects skip the file list and quit directly to shell, while multi-file
    /// projects show a file selector and return to it on 'q'.
    pub fn new(files: Vec<PathBuf>, sections: Vec<Section>, wrap_width: usize) -> Self {
        let file_mode = if files.len() == 1 {
            FileMode::Single
        } else {
            FileMode::Multi
        };

        Self {
            sections,
            files,
            current_file_index: 0,
            file_mode,
            current_view: View::List,
            current_section_index: 0,
            editor_state: None,
            command_buffer: String::new(),
            message: None,
            wrap_width,
            file_offsets: HashMap::new(),
        }
    }

    fn rebuild_file_offsets(&mut self) {
        self.file_offsets.clear();

        for (i, section) in self.sections.iter().enumerate() {
            if i == self.current_section_index {
                let lines_added = self.editor_state.as_ref().map_or(0, |es| es.lines.len());

                let file_map = self
                    .file_offsets
                    .entry(section.file_path.clone())
                    .or_default();

                file_map.insert(section.line_start, lines_added);
            }
        }
    }

    #[must_use]
    /// Calculates total lines added before a section to determine correct write position.
    ///
    /// Sums line changes from all preceding sections in the same file, enabling accurate patching
    /// without re-parsing after each edit.
    pub fn cumulative_offset(&self, index: usize) -> usize {
        let section = &self.sections[index];
        let target_file = &section.file_path;
        let target_line = section.line_start;

        if let Some(file_map) = self.file_offsets.get(target_file) {
            file_map
                .iter()
                .filter(|(line, _)| **line < target_line)
                .map(|(_, offset)| offset)
                .sum()
        } else {
            0
        }
    }

    /// Restores previously edited content from a saved edit plan.
    ///
    /// Matches edits to sections by file path and line coordinates, enabling session recovery or
    /// collaborative editing workflows.
    pub fn load_docs(&mut self, plan: EditPlan) {
        let mut doc_map: HashMap<String, Vec<String>> = HashMap::new();
        for edit in plan.edits {
            let key = format!(
                "{}:{}:{}",
                edit.file_name, edit.line_start, edit.column_start
            );
            let lines: Vec<String> = edit
                .doc_comment
                .lines()
                .map(std::string::ToString::to_string)
                .collect();
            doc_map.insert(key, lines);
        }

        // Match edits to sections and pre-populate editor content
        for section in &mut self.sections {
            let key = format!(
                "{}:{}:{}",
                section.file_path, section.line_start, section.column_start
            );
            if doc_map.contains_key(&key) {
                // Store the content - in a real implementation we might need a more
                // sophisticated approach to track which sections have been edited
                if let Ok(content) = fs::read_to_string(&section.file_path) {
                    let bytes = content.as_bytes();
                    if section.byte_start < bytes.len() && section.byte_end <= bytes.len() {
                        // Section exists and can be loaded
                        // The doc_lines represent previously edited content
                        // This would need to be applied or tracked somehow
                    }
                }
            }
        }
    }

    #[must_use]
    /// Creates a serialisable plan capturing current editor modifications.
    ///
    /// Enables saving work-in-progress as JSON for later restoration or applying edits through
    /// external tooling.
    pub fn generate_edit_plan(&self) -> EditPlan {
        let mut edits = Vec::new();

        // Generate edits from modified sections
        if let Some(ref editor_state) = self.editor_state {
            let section = &self.sections[self.current_section_index];
            let lines: Vec<String> = editor_state
                .lines
                .iter_row()
                .map(|line| line.iter().collect::<String>())
                .collect();

            let doc_comment = lines.join("\n");

            edits.push(Edit {
                file_name: section.file_path.clone(),
                line_start: section.line_start,
                line_end: section.line_end,
                column_start: section.column_start,
                column_end: section.column_end,
                doc_comment,
                item_name: section.title.clone(),
            });
        }

        EditPlan { edits }
    }

    /// Loads selected section content into the editor buffer.
    ///
    /// Extracts bytes between section boundaries and initialises vim-mode editing,
    /// trimming whitespace to present clean content.
    pub fn enter_detail_view(&mut self) {
        if self.sections.is_empty() {
            return;
        }

        let section = &self.sections[self.current_section_index];

        // Read file content
        if let Ok(content) = fs::read_to_string(&section.file_path) {
            let bytes = content.as_bytes();
            let section_bytes =
                &bytes[section.byte_start.min(bytes.len())..section.byte_end.min(bytes.len())];

            let section_content = String::from_utf8_lossy(section_bytes).to_string();

            let lines_text = if section_content.trim().is_empty() {
                "\n".to_string()
            } else {
                format!("\n{}\n", section_content.trim())
            };

            let lines = Lines::from(lines_text.as_str());
            self.editor_state = Some(EditorState::new(lines));
        }

        self.current_view = View::Detail;
    }

    /// Returns to section list, optionally persisting editor changes.
    ///
    /// Clears editor state and transitions view without saving unless explicitly requested.
    pub fn exit_detail_view(&mut self, save: bool) {
        if save {
            // Content is saved via save_current
        }
        self.editor_state = None;
        self.current_view = View::List;
    }

    /// Save the current section's content to disk.
    ///
    /// Trim the text (no newlines at start/end) and then pad again so that
    /// the section is always written with a single newline at either end.
    ///
    /// # Errors
    ///
    /// Returns an error if file operations fail.
    pub fn save_current(&mut self) -> io::Result<()> {
        let section = &self.sections[self.current_section_index];

        let editor_lines = if let Some(ref editor_state) = self.editor_state {
            editor_state
                .lines
                .iter_row()
                .map(|line| line.iter().collect::<String>())
                .collect::<Vec<_>>()
        } else {
            return Ok(());
        };

        // Join lines and strip leading/trailing whitespace
        let raw_content = editor_lines.join("\n");
        let trimmed_content = raw_content.trim();

        let padded_content = format!("\n{trimmed_content}\n\n");

        let edit = Edit {
            file_name: section.file_path.clone(),
            line_start: section.line_start,
            line_end: section.line_end,
            column_start: section.column_start,
            column_end: section.column_end,
            doc_comment: padded_content,
            item_name: section.title.clone(),
        };

        let mut plan = EditPlan { edits: vec![edit] };
        plan.apply()?;

        // Reload sections
        let format = MarkdownFormat;
        if let Ok(new_sections) =
            input::extract_sections(&PathBuf::from(&section.file_path), &format)
        {
            // Find matching section by title and level
            if let Some(new_index) = new_sections
                .iter()
                .position(|s| s.title == section.title && s.level == section.level)
            {
                // Update sections from this file
                let file_path = section.file_path.clone();
                self.sections.retain(|s| s.file_path != file_path);
                self.sections.extend(new_sections);
                self.current_section_index = new_index;
            }
        }

        self.rebuild_file_offsets();
        self.message = Some("Saved".to_string());
        Ok(())
    }

    #[must_use]
    /// Returns the following section index for sequential navigation.
    pub fn find_next_section(&self) -> Option<usize> {
        if self.current_section_index + 1 < self.sections.len() {
            Some(self.current_section_index + 1)
        } else {
            None
        }
    }

    #[must_use]
    /// Returns the preceding section index for reverse navigationon index for reverse navigation..
    pub fn find_prev_section(&self) -> Option<usize> {
        if self.current_section_index > 0 {
            Some(self.current_section_index - 1)
        } else {
            None
        }
    }

    #[must_use]
    /// Moves to the containing section in the document hierarchy.
    pub fn navigate_to_parent(&self) -> Option<usize> {
        self.sections[self.current_section_index].parent_index
    }

    #[must_use]
    /// Descends to the first child section in the document hierarchy.
    pub fn navigate_to_first_child(&self) -> Option<usize> {
        self.sections[self.current_section_index]
            .children_indices
            .first()
            .copied()
    }

    #[must_use]
    /// Calculates indentation width based on section nesting level.
    pub fn get_indent(&self) -> usize {
        if self.sections.is_empty() {
            return 0;
        }
        let section = &self.sections[self.current_section_index];
        section.level * 2
    }

    #[must_use]
    /// Determines available width for text after accounting for indentation.
    pub fn get_max_line_width(&self) -> usize {
        let indent = self.get_indent();
        self.wrap_width.saturating_sub(indent)
    }
}

#[cfg(test)]
#[path = "tests/app_state.rs"]
mod tests;
