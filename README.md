# EternaLibre Notes

**Free notes, forever.**

A server-side rendered (SSR) Markdown notes application written in Rust.

EternaLibre Notes combines **GNU software freedom** (GPL v3.0-or-later) with the
fluid elegance of Apple Notes and the predictable utility of classic desktop
GUI applications: a menu bar, split-pane editor, and file-based note storage.

It is a toolkit for making a **private notes cloud**. You run the server
yourself, on your own hardware, and the notes go nowhere else. The server
binds to all interfaces by default, so you can reach your notes from a phone or
another machine on your network, protected by a TOTP code from your
authenticator app. No telemetry, no accounts, no third-party services.

## Features

- **TOTP-locked by default** — authenticator app required on first launch and after every restart
- **Server-side rendered** with Axum and Maud — fast, type-safe HTML templates
- **Works without JavaScript** — note editing, search, theming, and the login form are plain HTML; the menus, mobile tabs, and live preview are the JS-dependent parts
- **Alpine.js enhancements** — live Markdown preview, instant search, keyboard shortcuts
- **File-based storage** — notes are persisted as Markdown files in `notes/`
- **Virtual folders** — organize notes into folders without real directories; folders are encoded as `[Folder]uuid.md` filename prefixes
- **Per-note exports** — download Markdown or self-contained themed HTML, or print/save the themed note as PDF
- **40 built-in themes** — including KDE Breeze, Blueberry Professional, classic desktop homages, distro-inspired palettes, warm neutrals, and more
- **Markdown extras** — tables, tasklists, strikethrough, and footnotes via `pulldown-cmark`
- **Syntax highlighting** — code blocks are highlighted using the current theme's ANSI palette via `syntect`
- **Mobile responsive** — collapses to a Notes/Write/Preview tab layout on phones, with 44px touch targets

## Quick Start

```bash
# Build and run (listens on 0.0.0.0:3000 by default)
cargo run

# Use a custom host/port
cargo run -- --host 127.0.0.1 --port 8080

# Serve behind a reverse proxy at a subdirectory
cargo run -- --base-path /el-notes
```

Then open http://127.0.0.1:3000 (or your chosen address).

Notes are stored in the `notes/` directory as `.md` files with YAML frontmatter.

## Folders

Notes can be organized into virtual folders without creating real directories on disk. A folder is encoded as a filename prefix:

```
[Project Notes]550e8400-e29b-41d4-a716-446655440000.md
```

Root notes (no folder) keep the plain `uuid.md` format. Folders exist only when they contain at least one note — there is no separate folder metadata to manage.

**Creating a folder:** click `+ New Folder` in the sidebar (or File → New Folder in the menu), type a name, and the app creates a new note in that folder and opens it.

**Filtering:** click a folder name in the sidebar to see only that folder's notes. Click **All Notes** to see everything grouped by folder. When a folder is active, the **+ New Note** button creates notes in that folder.

**Validation:** folder names reject path traversal (`..`), shell metacharacters (`;`, `|`, `&`, `$`, backtick, parens, angle brackets), control characters, leading dots, and names over 100 characters. Invalid names show a red flash message at the top of the page.

## Authentication

EternaLibre Notes is locked with TOTP. Because the server binds `0.0.0.0` by default, a code is required before any note can be read.

**First launch:** the server prints a QR code to your terminal and waits for a 6-digit code. Scan it with any TOTP authenticator (FreeOTP, Aegis, Bitwarden, Google Authenticator, …), or type the Base32 secret by hand if you would rather not scan. Only after a valid code is the secret committed to `.totp_secret` in the project root with mode `0600` — and only then does the port start listening.

Enrollment deliberately happens in the terminal rather than on a web page. There is no `/setup` route to reach: an HTTP-reachable setup endpoint would let anyone who can reach the port before you finish claim the device.

**Every launch after that:** you land on `/login` and need a current code. Sessions last one hour and live only in memory, so restarting the server invalidates them.

**Lost your authenticator?** Delete the secret and restart:

```bash
rm .totp_secret
```

There is no recovery-code fallback by design — that file is the only thing holding the secret, and it stays on your machine.

## Behind a reverse proxy

To serve at a subdirectory, pass `--base-path`:

```bash
cargo run -- --base-path /el-notes
```

```apache
ProxyPass        /el-notes/  http://127.0.0.1:3000/  timeout=10
ProxyPassReverse /el-notes/  http://127.0.0.1:3000/
```

The proxy strips the prefix, so the app still sees ordinary paths and adds the prefix back when generating links, form actions, and redirects. The session cookie's `Path` is scoped to the prefix, so the token is not sent to sibling apps on the same host.

## Command Line Options

```
Usage: eternalibre-notes [OPTIONS]

Options:
      --host <HOST>            Host address to bind to [default: 0.0.0.0]
  -p, --port <PORT>            Port to listen on [default: 3000]
      --notes-dir <NOTES_DIR>  Directory holding note files [default: notes]
      --base-path <BASE_PATH>  Path prefix when behind a reverse proxy subdirectory
  -h, --help                   Print help
```

## Development

```bash
# Run tests
cargo test

# Run clippy with warnings as errors
cargo clippy -- -D warnings

# Format code
cargo fmt
```

## Project Structure

| Path | Purpose |
|------|---------|
| `src/main.rs` | Server entry point, CLI parsing, first-run terminal setup |
| `src/lib.rs` | Library root, so `tests/` can drive the HTTP layer |
| `src/auth.rs` | TOTP secret, session store, and verification |
| `src/setup.rs` | First-run terminal enrollment |
| `src/routes.rs` | HTTP routes, auth gate, base-path joining |
| `src/pages.rs` | Maud SSR templates and URL generation |
| `src/notes.rs` | Note model and file-system persistence |
| `src/markdown.rs` | Markdown to HTML with syntax highlighting |
| `src/syntax.rs` | syntect integration and per-theme syntax color scheme generation |
| `src/themes.rs` | 40 predefined themes and CSS variable engine |
| `static/style.css` | Full application stylesheet |
| `static/app.js` | Alpine.js component: preview, search, menus, tabs |
| `tests/auth_gating.rs` | End-to-end tests for the TOTP auth gate |

## License

This program is free software: you can redistribute it and/or modify it under
the terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. See `LICENSE` for the full text.

