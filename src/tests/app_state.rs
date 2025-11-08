use super::AppState;
use crate::section::Section;
use std::fs;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_edit_persists_correctly() {
    // Create a test file
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "# Hello\n\n?\n\n## World\n\n??").unwrap();
    let path = file.path().to_path_buf();

    // Create sections matching the file
    let sections = vec![
        Section {
            title: "Hello".to_string(),
            level: 1,
            line_start: 2,
            line_end: 4,
            column_start: 0,
            column_end: 7,
            byte_start: 10, // after "# Hello\n\n"
            byte_end: 12,   // before "\n\n## World"
            file_path: path.to_string_lossy().to_string(),
            parent_index: None,
            children_indices: vec![1],
        },
        Section {
            title: "World".to_string(),
            level: 2,
            line_start: 5,
            line_end: 6,
            column_start: 1,
            column_end: 8,
            byte_start: 23,
            byte_end: 25,
            file_path: path.to_string_lossy().to_string(),
            parent_index: Some(0),
            children_indices: vec![],
        },
    ];

    let mut app = AppState::new(vec![path.clone()], sections, 100);

    // Enter detail view for first section
    app.enter_detail_view();

    // Simulate editing: replace "?" with "Yeah"
    if let Some(ref mut editor_state) = app.editor_state {
        editor_state.lines = edtui::Lines::from("\nYeah\n");
    }

    // Save
    app.save_current().unwrap();

    // Verify file content
    let content = fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = content.lines().collect();

    assert_eq!(lines[0], "# Hello");
    assert_eq!(lines[2], "Yeah", "Content should be 'Yeah', not '?'");
    assert_eq!(lines[4], "## World");
}

#[test]
fn test_edit_plan_captures_changes() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "# Test\n\nOriginal").unwrap();
    let path = file.path().to_path_buf();

    let sections = vec![Section {
        title: "Test".to_string(),
        level: 1,
        line_start: 3,
        line_end: 4,
        column_start: 1,
        column_end: 7,
        byte_start: 9,
        byte_end: 17,
        file_path: path.to_string_lossy().to_string(),
        parent_index: None,
        children_indices: vec![],
    }];

    let mut app = AppState::new(vec![path.clone()], sections, 100);

    app.enter_detail_view();

    // Make an edit
    if let Some(ref mut editor_state) = app.editor_state {
        editor_state.lines = edtui::Lines::from("\nModified\n");
    }

    app.save_current().unwrap();
    app.exit_detail_view(true);

    // Generate plan
    let plan = app.generate_edit_plan();

    assert!(
        !plan.edits.is_empty(),
        "Edit plan should contain the saved edit"
    );
    assert_eq!(plan.edits[0].doc_comment, "Modified");
}

#[test]
fn test_multiple_edits_correct_offsets() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "# One\n\nA\n\n## Two\n\nB\n\n### Three\n\nC").unwrap();
    let path = file.path().to_path_buf();

    let sections = vec![
        Section {
            title: "One".to_string(),
            level: 1,
            line_start: 3,
            line_end: 5,
            column_start: 1,
            column_end: 6,
            byte_start: 8,
            byte_end: 10,
            file_path: path.to_string_lossy().to_string(),
            parent_index: None,
            children_indices: vec![1],
        },
        Section {
            title: "Two".to_string(),
            level: 2,
            line_start: 6,
            line_end: 8,
            column_start: 1,
            column_end: 7,
            byte_start: 19,
            byte_end: 21,
            file_path: path.to_string_lossy().to_string(),
            parent_index: Some(0),
            children_indices: vec![2],
        },
        Section {
            title: "Three".to_string(),
            level: 3,
            line_start: 9,
            line_end: 11,
            column_start: 1,
            column_end: 10,
            byte_start: 33,
            byte_end: 35,
            file_path: path.to_string_lossy().to_string(),
            parent_index: Some(1),
            children_indices: vec![],
        },
    ];

    let mut app = AppState::new(vec![path.clone()], sections, 100);

    // Edit first section: A -> AAA (adds 2 lines)
    app.current_section_index = 0;
    app.enter_detail_view();
    if let Some(ref mut editor_state) = app.editor_state {
        editor_state.lines = edtui::Lines::from("\nAAA\n");
    }
    app.save_current().unwrap();
    app.exit_detail_view(true);

    // Edit third section: C -> CCC
    app.current_section_index = 2;
    app.enter_detail_view();
    if let Some(ref mut editor_state) = app.editor_state {
        editor_state.lines = edtui::Lines::from("\nCCC\n");
    }
    app.save_current().unwrap();

    // Verify file content
    let content = fs::read_to_string(&path).unwrap();
    assert!(
        content.contains("AAA"),
        "First edit should persist: {content}"
    );
    assert!(
        content.contains("CCC"),
        "Second edit should be at correct position: {content}"
    );
    assert!(content.contains("## Two"), "Middle section should remain");
}
