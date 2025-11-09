# ⁂  asterism

[![crates.io](https://img.shields.io/crates/v/asterism.svg)](https://crates.io/crates/asterism)
[![documentation](https://docs.rs/asterism/badge.svg)](https://docs.rs/asterism)
[![MIT licensed](https://img.shields.io/crates/l/asterism.svg)](./LICENSE)
[![pre-commit.ci status](https://results.pre-commit.ci/badge/github/lmmx/asterism/master.svg)](https://results.pre-commit.ci/latest/github/lmmx/asterism/master)

Hyperbolic navigation for tree data

## vim-like document section editor

asterism uses ratatui to give a hierarchical tree navigator for markdown documents,
and [edtui][edtui] to emulate a vim editor in which to edit section content.

[edtui]: https://github.com/preiter93/edtui

<img width="1769" height="907" alt="You are here" src="https://github.com/user-attachments/assets/35b0e7b5-66dd-410b-9625-98ae858bebe3" />

> You are here

## Features

- Tree-sitter based markdown parsing
- Hierarchical section navigation with parent/child relationships
- Section reordering from the header outline
- Visual line wrapping with configurable width
- Multi-file support with directory tree display
- Edit sections without modifying heading markup
- Vim-like keybindings for efficient editing

## Installation

Regular cargo install or (recommended) install the pre-built binary with:
```sh
cargo binstall asterism
```

## Usage

Edit markdown files in the current directory:
```sh
asterism
```

Edit a specific file:
```sh
asterism README.md
```

## Keybindings

### List View

- <kbd>↑</kbd>/<kbd>↓</kbd>: Jump to previous/next sections
  - <kbd>Shift</kbd> + <kbd>↑</kbd>/<kbd>↓</kbd>: Jump to previous/next section at same level
- <kbd>←</kbd>/<kbd>→</kbd>: Jump to parent section/next descendant
- <kbd>Home</kbd>/<kbd>End</kbd>: Jump to first/last section in document
  - <kbd>Shift</kbd> + <kbd>Home</kbd>/<kbd>End</kbd>: Jump to first/last section at same level
- <kbd>Enter</kbd>: Edit section
- <kbd>q</kbd>: Quit (or return to file list in multi-file mode)

#### Section Reordering

- <kbd>Ctrl</kbd> + <kbd>↑</kbd>/<kbd>↓</kbd>/<kbd>←</kbd>/<kbd>→</kbd>: Activate move mode (section turns orange), then move section up/down/in/out
- <kbd>Ctrl</kbd> + <kbd>←</kbd>/<kbd>→</kbd>: Change heading level (dedent/indent)
- <kbd>Ctrl</kbd> + <kbd>Home</kbd>/<kbd>End</kbd>: Move section to top/bottom of document
- <kbd>:w</kbd>: Save reordered structure to disk
- <kbd>Esc</kbd>: Cancel move operation

When moving, the selected section displays in orange, then turns red after being repositioned to indicate unsaved changes.

### Editor View

- <kbd>:w</kbd>: Save
- <kbd>:x</kbd>: Save and exit
- <kbd>:q</kbd>: Quit (warns if unsaved)
- <kbd>:q!</kbd>: Force quit
- <kbd>:wn</kbd>: Save and go to next header
- <kbd>:wp</kbd>: Save and go to previous section
- Standard vim editing commands

## Configuration

Create an `asterism.toml` file in your project directory:
```toml
wrap_width = 100
file_extensions = ["md", "markdown"]
```

## Licensing

Asterism is [MIT licensed](https://github.com/lmmx/asterism/blob/master/LICENSE), a permissive open source license.
