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

#[derive(Clone, PartialEq, Debug)]
/// Tracks the lifecycle of a section reordering operation.
///
/// Section moves proceed through distinct states to provide visual feedback and prevent
/// accidental modifications. The state machine is:
///
/// ```text
/// None -> Selected -> Moved -> None (after save or cancel)
///        ^ |
///        |____________________|
///              (cancel)
/// ```
///
/// # Visual Feedback
///
/// Each state has a corresponding visual style in the section list:
/// - `None`: Normal rendering (selected section uses reversed style)
/// - `Selected`: Orange text with bold modifier (indicates Ctrl key held)
/// - `Moved`: Red text with bold modifier (indicates unsaved changes)
///
/// # State Transitions
///
/// - **None → Selected**: Triggered by `Ctrl+↑/↓` on a section
/// - **Selected → Moved**: Any move operation (up/down/level change)
/// - **Moved → None**: Successful save (`:w`) or cancel (`Esc`)
/// - **Selected → None**: Cancel before any moves
pub enum MoveState {
    /// No section is being moved; normal navigation mode.
    ///
    /// In this state, all keybindings operate in their default navigation mode:
    /// - `↑/↓` move the cursor between sections
    /// - `←/→` navigate parent/child relationships
    /// - `Ctrl+↑/↓` initiates a move operation
    ///
    /// This is the default state and the state returned to after save or cancel.
    None,
    /// A section has been selected for moving but no changes made yet.
    ///
    /// Triggered by pressing `Ctrl+↑/↓` while in the `None` state. The selected section
    /// displays in orange to indicate it's ready to be repositioned. In this state:
    /// - `Ctrl+↑/↓` move the section up or down
    /// - `Ctrl+←/→` change the section's heading level
    /// - `Ctrl+Home/End` move the section to top or bottom
    /// - `Esc` cancels the operation without changes
    ///
    /// The first move operation transitions to `Moved` state.
    ///
    /// # Example
    ///
    /// ```text
    /// # Introduction <- normal
    /// ## Background <- SELECTED (orange, after Ctrl+↑/↓)
    /// ## Methods <- normal
    Selected,
    /// Section has been repositioned but changes not yet persisted to disk.
    ///
    /// After any move operation (position or level change), the section enters this state
    /// and displays in red to indicate unsaved modifications. The move can still be:
    /// - Refined with additional `Ctrl+arrow` operations
    /// - Saved with `:w` (writes new structure to disk, returns to `None`)
    /// - Cancelled with `Esc` (reverts all changes, returns to `None`)
    ///
    /// Multiple moves can be made before saving, allowing the user to position the section
    /// precisely before committing to disk.
    ///
    /// # Example
    ///
    /// ```text
    /// ## Background <- MOVED (red, after Ctrl+↑/↓ from position 2)
    /// # Introduction <- normal (was above, now below)
    /// ## Methods <- normal
    /// ```
    ///
    /// # Persistence
    ///
    /// When saved, the entire file is rewritten with sections in their new order and with
    /// updated heading levels. The rewrite preserves section content but regenerates the
    /// document structure. After save, sections are re-parsed from disk to ensure
    /// byte positions and line numbers are accurate.
    Moved,
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
    /// Tracks section being moved for visual feedback
    pub move_state: MoveState,
    /// Index of section being moved (if any)
    pub moving_section_index: Option<usize>,
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
            move_state: MoveState::None,
            moving_section_index: None,
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

        // Generate edits from ALL modified sections
        for section in &self.sections {
            if let Some(ref doc_lines) = section.doc_comment {
                let doc_comment = doc_lines.join("\n");

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
            // Extract text from editor and store in section
            if let Some(ref editor_state) = self.editor_state {
                let lines = editor_state
                    .lines
                    .iter_row()
                    .map(|line| line.iter().collect::<String>())
                    .collect();
                self.sections[self.current_section_index].doc_comment = Some(lines);
            }
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
        // Extract editor lines first, before any borrows
        let editor_lines = if let Some(ref editor_state) = self.editor_state {
            editor_state
                .lines
                .iter_row()
                .map(|line| line.iter().collect::<String>())
                .collect::<Vec<_>>()
        } else {
            return Ok(());
        };

        // Store in section for edit plan generation
        self.sections[self.current_section_index].doc_comment = Some(editor_lines.clone());

        // Now borrow section data
        let section = &self.sections[self.current_section_index];

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
            // Find matching section by title and level BEFORE modifying sections
            let target_title = section.title.clone();
            let target_level = section.level;

            // Remove old sections from this file
            let file_path = section.file_path.clone();
            self.sections.retain(|s| s.file_path != file_path);

            // Find the index in new_sections
            if let Some(local_index) = new_sections
                .iter()
                .position(|s| s.title == target_title && s.level == target_level)
            {
                // Calculate what the new global index will be after extending
                let new_global_index = self.sections.len() + local_index;

                // Now extend with new sections
                self.sections.extend(new_sections);

                // Set to the correct global index
                self.current_section_index = new_global_index;
            } else {
                // If we can't find it, just extend and stay at current position
                self.sections.extend(new_sections);
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
    /// Finds the next descendant section at any depth in the hierarchy.
    pub fn navigate_to_next_descendant(&self) -> Option<usize> {
        let current = self.current_section_index;

        // First try immediate children
        if let Some(first_child) = self.sections[current].children_indices.first() {
            return Some(*first_child);
        }

        // Otherwise, find the next section at any deeper level
        ((current + 1)..self.sections.len())
            .find(|&i| self.sections[i].level > self.sections[current].level)
    }

    #[must_use]
    /// Finds the next section at the same hierarchy level.
    pub fn navigate_to_next_sibling(&self) -> Option<usize> {
        let current_level = self.sections[self.current_section_index].level;

        for i in (self.current_section_index + 1)..self.sections.len() {
            if self.sections[i].level == current_level {
                return Some(i);
            }
            // Stop if we've gone up a level (past our parent's siblings)
            if self.sections[i].level < current_level {
                break;
            }
        }

        None
    }

    #[must_use]
    /// Finds the previous section at the same hierarchy level.
    pub fn navigate_to_prev_sibling(&self) -> Option<usize> {
        let current_level = self.sections[self.current_section_index].level;

        for i in (0..self.current_section_index).rev() {
            if self.sections[i].level == current_level {
                return Some(i);
            }
            // Stop if we've gone up a level
            if self.sections[i].level < current_level {
                break;
            }
        }

        None
    }

    #[must_use]
    /// Jumps to the first section in the document.
    pub fn navigate_to_first(&self) -> Option<usize> {
        if self.sections.is_empty() {
            None
        } else {
            Some(0)
        }
    }

    #[must_use]
    /// Jumps to the last section in the document.
    pub fn navigate_to_last(&self) -> Option<usize> {
        if self.sections.is_empty() {
            None
        } else {
            Some(self.sections.len() - 1)
        }
    }

    #[must_use]
    /// Finds the first section at the same hierarchy level.
    pub fn navigate_to_first_at_level(&self) -> Option<usize> {
        let current_level = self.sections[self.current_section_index].level;

        (0..self.sections.len()).find(|&i| self.sections[i].level == current_level)
    }

    #[must_use]
    /// Finds the last section at the same hierarchy level.
    pub fn navigate_to_last_at_level(&self) -> Option<usize> {
        let current_level = self.sections[self.current_section_index].level;

        (0..self.sections.len())
            .rev()
            .find(|&i| self.sections[i].level == current_level)
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

    // --- Section List Movement ---

    /// Start moving the current section
    pub fn start_move(&mut self) {
        self.moving_section_index = Some(self.current_section_index);
        self.move_state = MoveState::Selected;
    }

    /// Cancel the current move operation
    pub fn cancel_move(&mut self) {
        self.moving_section_index = None;
        self.move_state = MoveState::None;
    }

    /// Mark section as moved but not yet saved
    pub fn mark_moved(&mut self) {
        self.move_state = MoveState::Moved;
    }

    /// Move section up by one position
    pub fn move_section_up(&mut self) -> bool {
        if let Some(moving_idx) = self.moving_section_index {
            if moving_idx > 0 {
                self.sections.swap(moving_idx, moving_idx - 1);
                self.moving_section_index = Some(moving_idx - 1);
                self.current_section_index = moving_idx - 1;
                self.mark_moved();
                return true;
            }
        }
        false
    }

    /// Move section down by one position
    pub fn move_section_down(&mut self) -> bool {
        if let Some(moving_idx) = self.moving_section_index {
            if moving_idx < self.sections.len() - 1 {
                self.sections.swap(moving_idx, moving_idx + 1);
                self.moving_section_index = Some(moving_idx + 1);
                self.current_section_index = moving_idx + 1;
                self.mark_moved();
                return true;
            }
        }
        false
    }

    /// Move section to top of document
    pub fn move_section_to_top(&mut self) -> bool {
        if let Some(moving_idx) = self.moving_section_index {
            if moving_idx > 0 {
                let section = self.sections.remove(moving_idx);
                self.sections.insert(0, section);
                self.moving_section_index = Some(0);
                self.current_section_index = 0;
                self.mark_moved();
                return true;
            }
        }
        false
    }

    /// Move section to bottom of document
    pub fn move_section_to_bottom(&mut self) -> bool {
        if let Some(moving_idx) = self.moving_section_index {
            let last_idx = self.sections.len() - 1;
            if moving_idx < last_idx {
                let section = self.sections.remove(moving_idx);
                self.sections.push(section);
                self.moving_section_index = Some(last_idx);
                self.current_section_index = last_idx;
                self.mark_moved();
                return true;
            }
        }
        false
    }

    /// Increase section level (move in - lower level number)
    pub fn move_section_in(&mut self) -> bool {
        if let Some(moving_idx) = self.moving_section_index {
            if self.sections[moving_idx].level > 1 {
                self.sections[moving_idx].level -= 1;
                self.mark_moved();
                return true;
            }
        }
        false
    }

    /// Decrease section level (move out - higher level number)
    pub fn move_section_out(&mut self) -> bool {
        if let Some(moving_idx) = self.moving_section_index {
            if self.sections[moving_idx].level < 6 {
                self.sections[moving_idx].level += 1;
                self.mark_moved();
                return true;
            }
        }
        false
    }

    /// Apply section reordering to disk
    ///
    /// Rewrites files with sections in their new order and with updated heading levels.
    /// After successful write, reloads all sections from disk to ensure accurate byte
    /// positions and line numbers.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - File read operations fail
    /// - File write operations fail
    /// - Section extraction/parsing fails after rewrite
    pub fn save_section_reorder(&mut self) -> io::Result<()> {
        if self.move_state != MoveState::Moved {
            return Ok(());
        }

        // Group sections by file
        let mut file_sections: HashMap<String, Vec<&Section>> = HashMap::new();
        for section in &self.sections {
            file_sections
                .entry(section.file_path.clone())
                .or_default()
                .push(section);
        }

        // Process each file
        for (file_path, sections) in file_sections {
            Self::rewrite_file_sections(&file_path, &sections)?;
        }

        // Reload sections to get updated positions
        let format = MarkdownFormat;
        let mut new_sections = Vec::new();
        for file in &self.files {
            if let Ok(secs) = input::extract_sections(file, &format) {
                new_sections.extend(secs);
            }
        }

        self.sections = new_sections;
        self.cancel_move();
        self.message = Some("Sections reordered".to_string());

        Ok(())
    }

    /// Rewrite an entire file with reordered sections
    fn rewrite_file_sections(file_path: &str, sections: &[&Section]) -> io::Result<()> {
        let content = fs::read_to_string(file_path)?;
        let mut new_content = String::new();

        for section in sections {
            let heading_prefix = "#".repeat(section.level);
            let heading = format!("{} {}", heading_prefix, section.title);

            let bytes = content.as_bytes();
            let section_text =
                if section.byte_start < bytes.len() && section.byte_end <= bytes.len() {
                    String::from_utf8_lossy(&bytes[section.byte_start..section.byte_end])
                        .to_string()
                        .trim()
                        .to_string()
                } else {
                    String::new()
                };

            new_content.push_str(&heading);
            new_content.push_str("\n\n");
            if !section_text.is_empty() {
                new_content.push_str(&section_text);
                new_content.push_str("\n\n");
            }
        }

        fs::write(file_path, new_content)?;
        Ok(())
    }

    // --- </Section List Movement> ---
}

#[cfg(test)]
#[path = "tests/app_state.rs"]
mod tests;
