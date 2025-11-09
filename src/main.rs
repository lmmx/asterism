//! asterism: A tree-sitter document section editor.
#![allow(clippy::multiple_crate_versions)]

use asterism::{app_state, config, edit_plan, formats, input, ui};
use clap::Parser;
use edtui::EditorEventHandler;
use ratatui::crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "asterism")]
#[command(about = "Hyperbolic navigation for tree data", long_about = None)]
struct Args {
    /// Files or directories to edit
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,

    /// Load edit plan from JSON file
    #[arg(long)]
    load_docs: Option<PathBuf>,

    /// File extensions to match
    #[arg(long, short = 'e', value_name = "EXT")]
    ext: Vec<String>,
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    let mut cfg = config::Config::load();

    // Override config with command line args
    if !args.ext.is_empty() {
        cfg.file_extensions = args.ext;
    }

    let documents = input::find_documents(args.paths, &cfg.file_extensions)?;

    if documents.is_empty() {
        eprintln!("No matching files found");
        return Ok(());
    }

    // Extract sections from all documents
    let format = formats::markdown::MarkdownFormat;
    let mut all_sections = Vec::new();

    for doc in &documents {
        if let Ok(sections) = input::extract_sections(doc, &format) {
            all_sections.extend(sections);
        }
    }

    if all_sections.is_empty() {
        eprintln!("No sections found in documents");
        return Ok(());
    }

    let mut state = app_state::AppState::new(documents, all_sections, cfg.wrap_width);

    // Load docs if specified
    if let Some(load_path) = args.load_docs {
        let file_content = std::fs::read_to_string(&load_path)?;
        let plan: edit_plan::EditPlan = serde_json::from_str(&file_content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        state.load_docs(plan);
    }

    run_tui(state, &cfg)
}

fn run_tui(mut app: app_state::AppState, cfg: &config::Config) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut editor_handler = EditorEventHandler::default();

    let result = run_app(&mut terminal, &mut app, cfg, &mut editor_handler);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {e}");
    } else {
        let plan = app.generate_edit_plan();
        let json = serde_json::to_string_pretty(&plan).map_err(io::Error::other)?;
        println!("{json}");
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut app_state::AppState,
    cfg: &config::Config,
    editor_handler: &mut EditorEventHandler,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app, cfg))?;

        if let Event::Key(key) = event::read()? {
            match app.current_view {
                app_state::View::FileList => match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Up => {
                        if app.current_file_index > 0 {
                            app.current_file_index -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if app.current_file_index < app.files.len() - 1 {
                            app.current_file_index += 1;
                        }
                    }
                    KeyCode::Enter => {
                        // Load sections for selected file
                        let format = formats::markdown::MarkdownFormat;
                        if let Ok(sections) =
                            input::extract_sections(&app.files[app.current_file_index], &format)
                        {
                            app.sections = sections;
                            app.current_section_index = 0;
                            app.current_view = app_state::View::List;
                        }
                    }
                    _ => {}
                },
                app_state::View::List => match key.code {
                    KeyCode::Char('q') => {
                        if app.file_mode == app_state::FileMode::Multi {
                            app.current_view = app_state::View::FileList;
                        } else {
                            return Ok(());
                        }
                    }
                    KeyCode::Up => {
                        if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                            // Ctrl+Up: Start move or move up
                            if app.move_state == app_state::MoveState::None {
                                app.start_move();
                            } else {
                                app.move_section_up();
                            }
                        } else if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                            // Shift+Up: Jump to previous sibling at same level
                            if let Some(prev_sibling) = app.navigate_to_prev_sibling() {
                                app.current_section_index = prev_sibling;
                            }
                        } else {
                            // Normal up: Previous section
                            if app.current_section_index > 0 {
                                app.current_section_index -= 1;
                            }
                        }
                    }
                    KeyCode::Down => {
                        if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                            // Ctrl+Down: Start move (if not moving) or move down
                            if app.move_state == app_state::MoveState::None {
                                app.start_move();
                            } else {
                                app.move_section_down();
                            }
                        } else if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                            // Shift+Down: Jump to next sibling at same level
                            if let Some(next_sibling) = app.navigate_to_next_sibling() {
                                app.current_section_index = next_sibling;
                            }
                        } else {
                            // Normal down: Next section
                            if app.current_section_index < app.sections.len() - 1 {
                                app.current_section_index += 1;
                            }
                        }
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                            // Ctrl+Left: Decrease level (move in)
                            if app.move_state != app_state::MoveState::None {
                                app.move_section_in();
                            }
                        } else if let Some(parent_idx) = app.navigate_to_parent() {
                            app.current_section_index = parent_idx;
                        }
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                            // Ctrl+Right: Increase level (move in)
                            if app.move_state != app_state::MoveState::None {
                                app.move_section_out();
                            }
                        } else if let Some(descendant_idx) = app.navigate_to_next_descendant() {
                            app.current_section_index = descendant_idx;
                        }
                    }
                    KeyCode::Home => {
                        if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                            // Ctrl+Home: Move to top
                            if app.move_state != app_state::MoveState::None {
                                app.move_section_to_top();
                            }
                        } else if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                            // Shift+Home: Jump to first section at same level
                            if let Some(first_at_level) = app.navigate_to_first_at_level() {
                                app.current_section_index = first_at_level;
                            }
                        } else {
                            // Home: Jump to first section
                            if let Some(first) = app.navigate_to_first() {
                                app.current_section_index = first;
                            }
                        }
                    }
                    KeyCode::End => {
                        if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                            // Ctrl+End: Move to bottom
                            if app.move_state != app_state::MoveState::None {
                                app.move_section_to_bottom();
                            }
                        } else if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                            // Shift+End: Jump to last section at same level
                            if let Some(last_at_level) = app.navigate_to_last_at_level() {
                                app.current_section_index = last_at_level;
                            }
                        } else {
                            // End: Jump to last section
                            if let Some(last) = app.navigate_to_last() {
                                app.current_section_index = last;
                            }
                        }
                    }
                    KeyCode::Esc => {
                        if app.move_state != app_state::MoveState::None {
                            app.cancel_move();
                        }
                    }
                    KeyCode::Char(':') => {
                        // Allow entering command mode from move state to save
                        app.current_view = app_state::View::Command;
                        app.command_buffer.clear();
                        app.message = None;
                    }
                    KeyCode::Enter => {
                        // Don't enter detail view while moving
                        if app.move_state == app_state::MoveState::None {
                            app.enter_detail_view();
                        }
                    }
                    _ => {}
                },
                app_state::View::Detail => match key.code {
                    KeyCode::Char(':') => {
                        if let Some(ref editor_state) = app.editor_state {
                            if editor_state.mode == edtui::EditorMode::Normal {
                                app.current_view = app_state::View::Command;
                                app.command_buffer.clear();
                                app.message = None;
                            } else {
                                editor_handler
                                    .on_key_event(key, app.editor_state.as_mut().unwrap());
                            }
                        }
                    }
                    KeyCode::Esc => {
                        if let Some(ref editor_state) = app.editor_state {
                            if editor_state.mode == edtui::EditorMode::Normal {
                                app.exit_detail_view(false);
                            } else {
                                editor_handler
                                    .on_key_event(key, app.editor_state.as_mut().unwrap());
                            }
                        }
                    }
                    _ => {
                        if let Some(ref mut editor_state) = app.editor_state {
                            editor_handler.on_key_event(key, editor_state);
                        }
                    }
                },
                app_state::View::Command => match key.code {
                    KeyCode::Char(c) => {
                        app.command_buffer.push(c);
                    }
                    KeyCode::Backspace => {
                        app.command_buffer.pop();
                    }
                    KeyCode::Enter => {
                        let cmd = app.command_buffer.clone();
                        app.current_view = app_state::View::List; // Always return to List view

                        match cmd.as_str() {
                            "w" => {
                                if app.move_state == app_state::MoveState::Moved {
                                    if let Err(e) = app.save_section_reorder() {
                                        app.message = Some(format!("Error saving: {e}"));
                                    }
                                } else if app.editor_state.is_some() {
                                    // Only save current section if we're in the editor
                                    if let Err(e) = app.save_current() {
                                        app.message = Some(format!("Error saving: {e}"));
                                    }
                                } else {
                                    app.message = Some("Nothing to save".to_string());
                                }
                            }
                            "x" => {
                                if app.move_state == app_state::MoveState::Moved {
                                    if let Err(e) = app.save_section_reorder() {
                                        app.message = Some(format!("Error saving: {e}"));
                                    }
                                } else if app.editor_state.is_some() {
                                    if let Err(e) = app.save_current() {
                                        app.message = Some(format!("Error saving: {e}"));
                                    } else {
                                        app.exit_detail_view(true);
                                    }
                                }
                            }
                            "q" | "q!" => {
                                if app.editor_state.is_some() {
                                    app.exit_detail_view(false);
                                } else if app.move_state != app_state::MoveState::None {
                                    app.cancel_move();
                                } else {
                                    // Quit application logic
                                    if app.file_mode == app_state::FileMode::Multi {
                                        app.current_view = app_state::View::FileList;
                                    } else {
                                        return Ok(());
                                    }
                                }
                            }
                            "wn" => {
                                if app.editor_state.is_some() {
                                    if let Err(e) = app.save_current() {
                                        app.message = Some(format!("Error saving: {e}"));
                                    } else if let Some(next) = app.find_next_section() {
                                        app.exit_detail_view(true);
                                        app.current_section_index = next;
                                        app.enter_detail_view();
                                    } else {
                                        app.message = Some("No more sections".to_string());
                                    }
                                }
                            }
                            "wp" => {
                                if app.editor_state.is_some() {
                                    if let Err(e) = app.save_current() {
                                        app.message = Some(format!("Error saving: {e}"));
                                    } else if let Some(prev) = app.find_prev_section() {
                                        app.exit_detail_view(true);
                                        app.current_section_index = prev;
                                        app.enter_detail_view();
                                    } else {
                                        app.message = Some("No previous sections".to_string());
                                    }
                                }
                            }
                            _ => {
                                app.message = Some(format!("Unknown command: {cmd}"));
                            }
                        }
                        app.command_buffer.clear();
                    }
                    KeyCode::Esc => {
                        app.current_view = app_state::View::List; // Return to List view, not Detail
                        app.command_buffer.clear();
                    }
                    _ => {}
                },
            }
        }
    }
}
