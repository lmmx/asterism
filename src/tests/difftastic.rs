use crate::formats::difftastic::parse_difftastic_json;

#[test]
fn test_parse_single_file_diff() {
    let json = r##"[{"chunks":[[{"lhs":{"line_number":0,"changes":[{"start":0,"end":1,"content":"#","highlight":"normal"},{"start":1,"end":2,"content":" ","highlight":"normal"},{"start":2,"end":3,"content":"1","highlight":"normal"}]},"rhs":{"line_number":0,"changes":[{"start":0,"end":1,"content":"#","highlight":"normal"},{"start":1,"end":2,"content":" ","highlight":"normal"},{"start":2,"end":3,"content":"I","highlight":"normal"}]}},{"lhs":{"line_number":2,"changes":[{"start":0,"end":1,"content":"?","highlight":"normal"}]},"rhs":{"line_number":2,"changes":[{"start":0,"end":1,"content":"!","highlight":"normal"}]}}]],"language":"Text","path":"test.md","status":"changed"}]"##;

    let sections = parse_difftastic_json(json).unwrap();

    assert!(!sections.is_empty(), "Should parse sections");
    assert_eq!(sections[0].level, 1, "File should be level 1");
    assert!(
        sections[0].title.contains("test.md"),
        "Should contain filename"
    );

    if sections.len() > 1 {
        assert_eq!(sections[1].level, 2, "Hunk should be level 2");
        assert!(sections[1].title.contains("Hunk"), "Should be a hunk");
    }
}

#[test]
fn test_parse_git_diff_output() {
    // This is the actual output from: DFT_DISPLAY=json git -c diff.external=difft diff
    // Note: it's a single object, not an array!
    let json = r##"{"chunks":[[{"lhs":{"line_number":0,"changes":[{"start":0,"end":1,"content":"#","highlight":"normal"},{"start":1,"end":2,"content":" ","highlight":"normal"},{"start":2,"end":3,"content":"1","highlight":"normal"}]},"rhs":{"line_number":0,"changes":[{"start":0,"end":1,"content":"#","highlight":"normal"},{"start":1,"end":2,"content":" ","highlight":"normal"},{"start":2,"end":3,"content":"I","highlight":"normal"}]}},{"lhs":{"line_number":2,"changes":[{"start":0,"end":1,"content":"?","highlight":"normal"}]},"rhs":{"line_number":2,"changes":[{"start":0,"end":1,"content":"!","highlight":"normal"}]}},{"lhs":{"line_number":4,"changes":[{"start":0,"end":1,"content":"#","highlight":"normal"},{"start":1,"end":2,"content":"#","highlight":"normal"},{"start":2,"end":3,"content":" ","highlight":"normal"},{"start":3,"end":4,"content":"2","highlight":"normal"}]},"rhs":{"line_number":4,"changes":[{"start":0,"end":1,"content":"#","highlight":"normal"},{"start":1,"end":2,"content":"#","highlight":"normal"},{"start":2,"end":3,"content":" ","highlight":"normal"},{"start":3,"end":5,"content":"II","highlight":"normal"}]}},{"lhs":{"line_number":6,"changes":[{"start":0,"end":1,"content":"?","highlight":"normal"},{"start":1,"end":2,"content":"?","highlight":"normal"}]},"rhs":{"line_number":6,"changes":[{"start":0,"end":1,"content":"!","highlight":"normal"},{"start":1,"end":2,"content":"!","highlight":"normal"}]}},{"lhs":{"line_number":8,"changes":[{"start":0,"end":1,"content":"#","highlight":"normal"},{"start":1,"end":2,"content":"#","highlight":"normal"},{"start":2,"end":3,"content":"#","highlight":"normal"},{"start":3,"end":4,"content":" ","highlight":"normal"},{"start":4,"end":5,"content":"3","highlight":"normal"}]},"rhs":{"line_number":8,"changes":[{"start":0,"end":1,"content":"#","highlight":"normal"},{"start":1,"end":2,"content":"#","highlight":"normal"},{"start":2,"end":3,"content":"#","highlight":"normal"},{"start":3,"end":4,"content":" ","highlight":"normal"},{"start":4,"end":7,"content":"III","highlight":"normal"}]}},{"lhs":{"line_number":10,"changes":[{"start":0,"end":1,"content":"?","highlight":"normal"},{"start":1,"end":2,"content":"?","highlight":"normal"},{"start":2,"end":3,"content":"?","highlight":"normal"}]},"rhs":{"line_number":10,"changes":[{"start":0,"end":1,"content":"!","highlight":"normal"},{"start":1,"end":2,"content":"!","highlight":"normal"},{"start":2,"end":3,"content":"!","highlight":"normal"}]}},{"lhs":{"line_number":11,"changes":[{"start":0,"end":0,"content":"","highlight":"normal"}]}}]],"language":"Text","path":"test.md","status":"changed"}"##;

    let sections = parse_difftastic_json(json).unwrap();

    assert!(
        !sections.is_empty(),
        "Should parse sections from git diff output"
    );
    assert_eq!(
        sections[0].title, "test.md (changed)",
        "Should parse file with status"
    );
}

#[test]
fn test_parse_unchanged_file() {
    let json = r#"[{"language":"Text","path":"unchanged.md","status":"unchanged"}]"#;

    let sections = parse_difftastic_json(json).unwrap();

    // Unchanged files should be skipped
    assert!(sections.is_empty(), "Should skip unchanged files");
}

#[test]
#[ignore]
fn test_parse_binary_file() {
    let json = r#"[{"language":"Binary","path":"image.png","status":"changed"}]"#;

    let sections = parse_difftastic_json(json).unwrap();

    // Binary files with no chunks should still create a file section
    assert!(!sections.is_empty(), "Should handle binary files");
    assert_eq!(sections[0].level, 1);
}
