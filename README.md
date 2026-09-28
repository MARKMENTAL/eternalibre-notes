# Rasuti Notes (ラスティ Notes)

A server-side rendered (SSR) Markdown notes application written in Rust.

Rasuti Notes combines **GNU software freedom** (GPL v3.0-or-later) with the
fluid elegance of Apple Notes and the predictable utility of classic desktop
GUI applications: a menu bar, split-pane editor, and file-based note storage.

## Features

- **Server-side rendered** with Axum and Maud — fast, type-safe HTML templates
- **Zero-JS fallback** — every action works via standard HTML forms
- **Alpine.js enhancements** — live Markdown preview, instant search, keyboard shortcuts
- **File-based storage** — notes are persisted as Markdown files in `notes/`
- **28 built-in themes** — from Dark/Light to Solarized, Dracula, Nord, Monokai, and seasonal palettes
- **Markdown extras** — tables, tasklists, strikethrough, and footnotes via `pulldown-cmark`

## Quick Start

```bash
# Build and run (listens on 0.0.0.0:3000 by default)
cargo run

# Use a custom host/port
cargo run -- --host 127.0.0.1 --port 8080
```

Then open http://127.0.0.1:3000 (or your chosen address).

Notes are stored in the `notes/` directory as `.md` files with YAML frontmatter.

## Development

```bash
# Run tests
cargo test

# Run clippy with warnings as errors
cargo clippy -- -D warnings

# Format code
cargo fmt
```

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl/Cmd + N` | Create a new note |
| `Ctrl/Cmd + S` | Save the current note |

## License

This program is free software: you can redistribute it and/or modify it under
the terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. See `LICENSE` for the full text.
