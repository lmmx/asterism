//! The UI renders the application state into something visible and vim-able.
//!
//! The draw function dispatches based on the current view (file list, section list, or editor).
//! The file list shows a directory tree for multi-file projects, the list view shows sections
//! with their hierarchy, and the detail view provides a vim-like editor for section content.

use crate::app_state::{AppState, FileMode, MoveState, View};
use crate::config::Config;
use crate::formats::Format;
use edtui::{EditorTheme, EditorView, SyntaxHighlighter};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

/// Renders the active view based on current application state.
///
/// Dispatches to file list, section tree, or editor view according to navigation context.
pub fn draw(f: &mut Frame, app: &mut AppState, _cfg: &Config) {
    match (&app.file_mode, &app.current_view) {
        (FileMode::Multi, View::List) if app.current_view == View::List => draw_file_list(f, app),
        _ => match app.current_view {
            View::List => draw_list(f, app),
            View::Detail | View::Command => draw_detail(f, app),
            View::FileList => draw_file_list(f, app),
        },
    }
}

fn draw_file_list(f: &mut Frame, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(f.area());

    let items: Vec<ListItem> = app
        .files
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let display = path.to_string_lossy().to_string();
            let style = if i == app.current_file_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            ListItem::new(display).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("Files ({})", app.files.len())),
    );

    f.render_widget(list, chunks[0]);

    let help = Paragraph::new("↑/↓: Navigate | Enter: Select File | q: Quit")
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(help, chunks[1]);
}

fn draw_list(f: &mut Frame, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(f.area());

    let format = crate::formats::markdown::MarkdownFormat;

    let items: Vec<ListItem> = app
        .sections
        .iter()
        .enumerate()
        .map(|(i, section)| {
            let indent = "  ".repeat(section.level.saturating_sub(1));
            let highlighted_line = format.format_section_display(section.level, &section.title);

            // Prepend indent as plain text
            let mut spans = vec![Span::raw(indent)];
            spans.extend(highlighted_line.spans);
            let line = Line::from(spans);

            // Determine style based on move state
            let style = if Some(i) == app.moving_section_index {
                match app.move_state {
                    MoveState::Selected => Style::default()
                        .fg(Color::Rgb(255, 165, 0)) // Orange
                        .add_modifier(Modifier::BOLD),
                    MoveState::Moved => {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                    }
                    MoveState::None => {
                        if i == app.current_section_index {
                            Style::default().add_modifier(Modifier::REVERSED)
                        } else {
                            Style::default()
                        }
                    }
                }
            } else if i == app.current_section_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            ListItem::new(line).style(style)
        })
        .collect();

    let title = if app.move_state == MoveState::None {
        "Sections"
    } else {
        "Sections (MOVING)"
    };

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));

    f.render_widget(list, chunks[0]);

    let help = if app.move_state == MoveState::None {
        "↑/↓: Navigate | ←/→: Parent/Child | Enter: Edit | Ctrl+↑: Start Move | q: Quit"
    } else {
        "Ctrl+↑/↓: Move | Ctrl+←/→: Level | Ctrl+Home/End: Top/Bottom | :w Save | Esc: Cancel"
    };

    let help_widget = Paragraph::new(help).block(Block::default().borders(Borders::ALL));
    f.render_widget(help_widget, chunks[1]);
}

fn draw_detail(f: &mut Frame, app: &mut AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Breadcrumb
            Constraint::Min(0),    // Editor
            Constraint::Length(3), // Help
        ])
        .split(f.area());

    // Breadcrumb navigation
    let section = &app.sections[app.current_section_index];
    let mut breadcrumb_parts = Vec::new();
    let mut current_idx = Some(app.current_section_index);

    while let Some(idx) = current_idx {
        breadcrumb_parts.push(app.sections[idx].title.clone());
        current_idx = app.sections[idx].parent_index;
    }

    breadcrumb_parts.reverse();
    let breadcrumb = breadcrumb_parts.join(" > ");

    let breadcrumb_widget = Paragraph::new(breadcrumb)
        .block(Block::default().borders(Borders::ALL).title("Navigation"));
    f.render_widget(breadcrumb_widget, chunks[0]);

    // Editor
    let max_width = app.get_max_line_width();
    let title = format!("Section: {} (max line: {} chars)", section.title, max_width);

    if let Some(ref mut editor_state) = app.editor_state {
        let block = Block::default().borders(Borders::ALL).title(title);
        let inner = block.inner(chunks[1]);
        f.render_widget(block, chunks[1]);

        let syntax_highlighter = SyntaxHighlighter::new("dracula", "md");
        let editor = EditorView::new(editor_state)
            .theme(EditorTheme::default())
            .syntax_highlighter(Some(syntax_highlighter))
            .wrap(true);

        f.render_widget(editor, inner);
    }

    // Help/command line
    let help_text = if app.current_view == View::Command {
        format!(":{}", app.command_buffer)
    } else if let Some(ref msg) = app.message {
        msg.clone()
    } else {
        ":w Save | :x Save & Exit | :q Quit | :q! Force Quit | :wn Save & Next | :wp Save & Prev"
            .to_string()
    };

    let help = Paragraph::new(help_text).block(Block::default().borders(Borders::ALL));
    f.render_widget(help, chunks[2]);
}
