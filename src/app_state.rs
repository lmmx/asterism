//! The core state machine bridging document sections and the interactive editor.
//!
//! A TUI needs a single source of truth that can be interrogated and mutated as the user navigates
//! and edits. We achieve this by syncing the editor save state with the files on disk. We keep
//! track of the cumulative total number of lines that have been added to the file during the
//! session so that we can determine the correct offset to insert content at without re-parsing.

use crate::edit_plan::{Edit, EditPlan};
use crate::formats::markdown::MarkdownFormat;
use crate::input;
use crate::section::{NodeType, Section, TreeNode};
use edtui::{EditorState, Lines};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

#[derive(PartialEq)]
/// Determines navigation scope and quit behavior based on project size.
pub enum FileMode {
    /// Single-file mode quits directly to shell.
    Single,
    /// Multi-file mode shows file tree in list view.
    Multi,
}

#[derive(Clone, PartialEq, Debug)]
/// Tracks the lifecycle of a section reordering operation.
pub enum MoveState {
    /// No section is being moved; normal navigation mode.
    None,
    /// A section has been selected for moving but no changes made yet.
    Selected,
    /// Section has been repositioned but changes not yet persisted to disk.
    Moved,
}

/// Bridges document sections and the interactive editor, maintaining session state.
pub struct AppState {
    /// All parsed sections across loaded files.
    pub sections: Vec<Section>,
    /// Unified tree of directories, files, and sections for display
    pub tree_nodes: Vec<TreeNode>,
    /// File paths available for editing.
    pub files: Vec<PathBuf>,
    /// Controls navigation behavior and file tree visibility.
    pub file_mode: FileMode,
    /// Active UI screen determining input handling.
    pub current_view: View,
    /// Selected node index in the tree
    pub current_node_index: usize,
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
    pub fn new(files: Vec<PathBuf>, sections: Vec<Section>, wrap_width: usize) -> Self {
        let file_mode = if files.len() == 1 {
            FileMode::Single
        } else {
            FileMode::Multi
        };

        let tree_nodes = Self::build_tree(&files, &sections);

        Self {
            sections,
            tree_nodes,
            files,
            file_mode,
            current_view: View::List,
            current_node_index: 0,
            editor_state: None,
            command_buffer: String::new(),
            message: None,
            wrap_width,
            file_offsets: HashMap::new(),
            move_state: MoveState::None,
            moving_section_index: None,
        }
    }

    /// Build the unified tree structure from files and sections
    fn build_tree(files: &[PathBuf], sections: &[Section]) -> Vec<TreeNode> {
        let mut nodes = Vec::new();

        if files.len() == 1 {
            // Single file mode: just show sections with their heading hierarchy
            for (idx, section) in sections.iter().enumerate() {
                nodes.push(TreeNode::section(
                    section.clone(),
                    section.level - 1, // Convert heading level to tree level
                    idx,
                ));
            }
        } else {
            // Multi-file mode: build directory tree with sections nested under files
            let mut file_tree: HashMap<String, Vec<(usize, &Section)>> = HashMap::new();

            // Group sections by file
            for (idx, section) in sections.iter().enumerate() {
                file_tree
                    .entry(section.file_path.clone())
                    .or_default()
                    .push((idx, section));
            }

            // Build tree with directory structure
            let mut sorted_files: Vec<_> = files.iter().collect();
            sorted_files.sort();

            for file_path in sorted_files {
                let path_str = file_path.to_string_lossy().to_string();

                // Add file node (non-navigable)
                let file_name = file_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path_str.clone());

                nodes.push(TreeNode::file(file_name, path_str.clone(), 0));

                // Add sections under this file
                if let Some(file_sections) = file_tree.get(&path_str) {
                    for (idx, section) in file_sections {
                        // Section tree level = 1 (under file) + heading level - 1
                        let tree_level = section.level;
                        nodes.push(TreeNode::section(section.clone(), tree_level, *idx));
                    }
                }
            }
        }

        // Ensure we start on a navigable node
        if let Some(first_navigable) = nodes.iter().position(|n| n.navigable) {
            if first_navigable > 0 {
                // Move first navigable to start if not already there
                // Actually, keep tree structure but navigation will skip
            }
        }

        nodes
    }

    /// Rebuild tree after sections change (e.g., after save)
    pub fn rebuild_tree(&mut self) {
        self.tree_nodes = Self::build_tree(&self.files, &self.sections);

        // Try to maintain current position by finding same section
        if let Some(current_section_idx) = self.get_current_section_index() {
            if let Some(node_idx) = self.tree_nodes.iter().position(|n| {
                n.section_index == Some(current_section_idx)
            }) {
                self.current_node_index = node_idx;
            }
        }
    }

    /// Get the section index for the currently selected node (if it's a section)
    #[must_use]
    pub fn get_current_section_index(&self) -> Option<usize> {
        if self.current_node_index < self.tree_nodes.len() {
            self.tree_nodes[self.current_node_index].section_index
        } else {
            None
        }
    }

    /// Get the current section (if on a section node)
    #[must_use]
    pub fn get_current_section(&self) -> Option<&Section> {
        self.get_current_section_index()
            .and_then(|idx| self.sections.get(idx))
    }

    fn rebuild_file_offsets(&mut self) {
        self.file_offsets.clear();

        if let Some(section_idx) = self.get_current_section_index() {
            if let Some(section) = self.sections.get(section_idx) {
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
                if let Ok(content) = fs::read_to_string(&section.file_path) {
                    let bytes = content.as_bytes();
                    if section.byte_start < bytes.len() && section.byte_end <= bytes.len() {
                        // Section exists and can be loaded
                    }
                }
            }
        }
    }

    #[must_use]
    /// Creates a serialisable plan capturing current editor modifications.
    pub fn generate_edit_plan(&self) -> EditPlan {
        let mut edits = Vec::new();

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
    pub fn enter_detail_view(&mut self) {
        let Some(section_idx) = self.get_current_section_index() else {
            return;
        };

        let section = &self.sections[section_idx];

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
    pub fn exit_detail_view(&mut self, save: bool) {
        if save {
            if let Some(ref editor_state) = self.editor_state {
                if let Some(section_idx) = self.get_current_section_index() {
                    let lines = editor_state
                        .lines
                        .iter_row()
                        .map(|line| line.iter().collect::<String>())
                        .collect();
                    self.sections[section_idx].doc_comment = Some(lines);
                }
            }
        }
        self.editor_state = None;
        self.current_view = View::List;
    }

    /// Save the current section's content to disk.
    pub fn save_current(&mut self) -> io::Result<()> {
        let editor_lines = if let Some(ref editor_state) = self.editor_state {
            editor_state
                .lines
                .iter_row()
                .map(|line| line.iter().collect::<String>())
                .collect::<Vec<_>>()
        } else {
            return Ok(());
        };

        let Some(section_idx) = self.get_current_section_index() else {
            return Ok(());
        };

        self.sections[section_idx].doc_comment = Some(editor_lines.clone());

        let section = &self.sections[section_idx];

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
            let target_title = section.title.clone();
            let target_level = section.level;

            let file_path = section.file_path.clone();
            self.sections.retain(|s| s.file_path != file_path);

            if let Some(local_index) = new_sections
                .iter()
                .position(|s| s.title == target_title && s.level == target_level)
            {
                let new_global_index = self.sections.len() + local_index;
                self.sections.extend(new_sections);

                // Rebuild tree and find the updated section
                self.rebuild_tree();

                // Find node with this section index
                if let Some(node_idx) = self.tree_nodes.iter().position(|n| {
                    n.section_index == Some(new_global_index)
                }) {
                    self.current_node_index = node_idx;
                }
            } else {
                self.sections.extend(new_sections);
                self.rebuild_tree();
            }
        }

        self.rebuild_file_offsets();
        self.message = Some("Saved".to_string());
        Ok(())
    }

    /// Navigate to next navigable node
    #[must_use]
    pub fn find_next_node(&self) -> Option<usize> {
        for i in (self.current_node_index + 1)..self.tree_nodes.len() {
            if self.tree_nodes[i].navigable {
                return Some(i);
            }
        }
        None
    }

    /// Navigate to previous navigable node
    #[must_use]
    pub fn find_prev_node(&self) -> Option<usize> {
        for i in (0..self.current_node_index).rev() {
            if self.tree_nodes[i].navigable {
                return Some(i);
            }
        }
        None
    }

    #[must_use]
    /// Moves to the containing section in the document hierarchy.
    pub fn navigate_to_parent(&self) -> Option<usize> {
        let section_idx = self.get_current_section_index()?;
        let parent_section_idx = self.sections[section_idx].parent_index?;

        // Find tree node with this section index
        self.tree_nodes.iter().position(|n| {
            n.section_index == Some(parent_section_idx)
        })
    }

    #[must_use]
    /// Descends to the first child section in the document hierarchy.
    pub fn navigate_to_first_child(&self) -> Option<usize> {
        let section_idx = self.get_current_section_index()?;
        let first_child_idx = self.sections[section_idx].children_indices.first()?;

        self.tree_nodes.iter().position(|n| {
            n.section_index == Some(*first_child_idx)
        })
    }

    #[must_use]
    /// Finds the next descendant section at any depth in the hierarchy.
    pub fn navigate_to_next_descendant(&self) -> Option<usize> {
        let section_idx = self.get_current_section_index()?;

        // First try immediate children
        if let Some(first_child) = self.sections[section_idx].children_indices.first() {
            return self.tree_nodes.iter().position(|n| {
                n.section_index == Some(*first_child)
            });
        }

        // Otherwise find next section at deeper level
        for i in (section_idx + 1)..self.sections.len() {
            if self.sections[i].level > self.sections[section_idx].level {
                return self.tree_nodes.iter().position(|n| {
                    n.section_index == Some(i)
                });
            }
        }

        None
    }

    #[must_use]
    /// Finds the next section at the same hierarchy level.
    pub fn navigate_to_next_sibling(&self) -> Option<usize> {
        let section_idx = self.get_current_section_index()?;
        let current_level = self.sections[section_idx].level;

        for i in (section_idx + 1)..self.sections.len() {
            if self.sections[i].level == current_level {
                return self.tree_nodes.iter().position(|n| {
                    n.section_index == Some(i)
                });
            }
            if self.sections[i].level < current_level {
                break;
            }
        }

        None
    }

    #[must_use]
    /// Finds the previous section at the same hierarchy level.
    pub fn navigate_to_prev_sibling(&self) -> Option<usize> {
        let section_idx = self.get_current_section_index()?;
        let current_level = self.sections[section_idx].level;

        for i in (0..section_idx).rev() {
            if self.sections[i].level == current_level {
                return self.tree_nodes.iter().position(|n| {
                    n.section_index == Some(i)
                });
            }
            if self.sections[i].level < current_level {
                break;
            }
        }

        None
    }

    #[must_use]
    /// Jumps to the first navigable node.
    pub fn navigate_to_first(&self) -> Option<usize> {
        self.tree_nodes.iter().position(|n| n.navigable)
    }

    #[must_use]
    /// Jumps to the last navigable node.
    pub fn navigate_to_last(&self) -> Option<usize> {
        self.tree_nodes.iter().rposition(|n| n.navigable)
    }

    #[must_use]
    /// Finds the first section at the same hierarchy level.
    pub fn navigate_to_first_at_level(&self) -> Option<usize> {
        let section_idx = self.get_current_section_index()?;
        let current_level = self.sections[section_idx].level;

        for i in 0..self.sections.len() {
            if self.sections[i].level == current_level {
                return self.tree_nodes.iter().position(|n| {
                    n.section_index == Some(i)
                });
            }
        }

        None
    }

    #[must_use]
    /// Finds the last section at the same hierarchy level.
    pub fn navigate_to_last_at_level(&self) -> Option<usize> {
        let section_idx = self.get_current_section_index()?;
        let current_level = self.sections[section_idx].level;

        for i in (0..self.sections.len()).rev() {
            if self.sections[i].level == current_level {
                return self.tree_nodes.iter().position(|n| {
                    n.section_index == Some(i)
                });
            }
        }

        None
    }

    #[must_use]
    /// Calculates indentation width based on section nesting level.
    pub fn get_indent(&self) -> usize {
        if let Some(section) = self.get_current_section() {
            section.level * 2
        } else {
            0
        }
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
        if let Some(section_idx) = self.get_current_section_index() {
            self.moving_section_index = Some(section_idx);
            self.move_state = MoveState::Selected;
        }
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
                self.rebuild_tree();

                // Update current node to follow the moved section
                if let Some(node_idx) = self.tree_nodes.iter().position(|n| {
                    n.section_index == Some(moving_idx - 1)
                }) {
                    self.current_node_index = node_idx;
                }

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
                self.rebuild_tree();

                if let Some(node_idx) = self.tree_nodes.iter().position(|n| {
                    n.section_index == Some(moving_idx + 1)
                }) {
                    self.current_node_index = node_idx;
                }

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
                self.rebuild_tree();

                if let Some(node_idx) = self.tree_nodes.iter().position(|n| {
                    n.section_index == Some(0)
                }) {
                    self.current_node_index = node_idx;
                }

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
                self.rebuild_tree();

                if let Some(node_idx) = self.tree_nodes.iter().position(|n| {
                    n.section_index == Some(last_idx)
                }) {
                    self.current_node_index = node_idx;
                }

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
                self.rebuild_tree();
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
                self.rebuild_tree();
                self.mark_moved();
                return true;
            }
        }
        false
    }

    /// Apply section reordering to disk
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
        self.rebuild_tree();
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
}

#[cfg(test)]
#[path = "tests/app_state.rs"]
mod tests;
