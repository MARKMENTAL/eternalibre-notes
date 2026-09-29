use crate::markdown::render_markdown;
use crate::notes::Note;
use crate::themes::{get_theme, PREDEFINED_THEMES};
use chrono::{DateTime, Utc};
use maud::{html, Markup, PreEscaped, DOCTYPE};

/// Base path prefix for generated URLs.
///
/// Wraps the configured prefix so every `href`/`action`/`src` in the templates
/// goes through one call. Empty means mounted at the root, which is the
/// default and what most installs use.
#[derive(Clone, Copy)]
pub struct Base<'a>(&'a str);

impl<'a> Base<'a> {
    pub fn new(prefix: &'a str) -> Self {
        Base(prefix)
    }

    /// Joins the prefix onto an app-absolute path.
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.0, path)
    }
}

pub fn render_app(
    theme_name: &str,
    notes: &[Note],
    current_note: Option<&Note>,
    query: Option<&str>,
    message: Option<&str>,
    base: &Base<'_>,
) -> Markup {
    let theme = get_theme(theme_name);
    let css = theme.to_css_block();
    let rendered_preview = current_note
        .map(|n| render_markdown(&n.content, theme))
        .unwrap_or_default();

    // Mobile lands on the note list when there is no note selected,
    // otherwise on the editor. Server-rendered so it works without JS.
    let default_tab = if current_note.is_some() {
        "write"
    } else {
        "notes"
    };

    html! {
        (DOCTYPE)
        html lang="en" data-theme=(theme_name) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover";
                title { "EternaLibre Notes" }
                style { (PreEscaped(css)) }
                style { (PreEscaped(include_str!("../static/style.css"))) }
                script defer src=(base.url("/static/alpine.min.js")) {}
            }
            body x-data="app()" x-on:keydown="handleKeydown($event)" {
                (render_menu_bar(base))
                (render_toolbar(current_note, query, theme_name, base))
                @if let Some(msg) = message {
                    div class="flash-message" { (msg) }
                }
                (render_tab_bar(default_tab))
                main class="app-body"
                    x-bind:class="{ 'hide-editor': !editorVisible, 'hide-preview': !previewVisible }" {
                    (render_sidebar(notes, current_note, default_tab, base))
                    (render_editor(current_note, default_tab, base))
                    (render_preview(&rendered_preview, default_tab))
                }
                (render_status_bar(notes.len(), theme_name))
                script {
                    "window.__INITIAL_PREVIEW__ = `" (PreEscaped(escape_js(&rendered_preview))) "`;"
                    (PreEscaped(include_str!("../static/app.js")))
                }
            }
        }
    }
}

fn escape_js(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn render_tab_bar(default_tab: &str) -> Markup {
    html! {
        nav class="tab-bar" {
            button type="button" class="tab-button" data-tab-target="notes"
                x-on:click="setTab('notes')"
                x-bind:class="{ 'active': tab === 'notes' }" {
                span class="tab-icon" { "\u{1F4C3}" }
                span { "Notes" }
            }
            button type="button" class="tab-button" data-tab-target="write"
                x-on:click="setTab('write')"
                x-bind:class="{ 'active': tab === 'write' }" {
                span class="tab-icon" { "\u{270E}" }
                span { "Write" }
            }
            button type="button" class="tab-button" data-tab-target="preview"
                x-on:click="setTab('preview')"
                x-bind:class="{ 'active': tab === 'preview' }" {
                span class="tab-icon" { "\u{1F4C4}" }
                span { "Preview" }
            }
            script {
                (PreEscaped(format!(
                    "window.__DEFAULT_TAB__ = \"{}\";",
                    default_tab
                )))
            }
        }
    }
}

fn render_menu_bar(base: &Base<'_>) -> Markup {
    html! {
        nav class="menu-bar" {
            ul class="menu-list" {
                li class="menu-item" tabindex="0"
                    x-on:click="toggleMenu('file')"
                    x-bind:class="{ 'open': openMenu === 'file' }" {
                    span { "File" }
                    ul class="menu-dropdown" {
                        li {
                            form action=(base.url("/notes")) method="post" {
                                button type="submit" { "New Note" }
                            }
                        }
                        li { hr; }
                        li {
                            a href=(base.url("/export")) { "Export All Notes" }
                        }
                    }
                }
                li class="menu-item" tabindex="0"
                    x-on:click="toggleMenu('edit')"
                    x-bind:class="{ 'open': openMenu === 'edit' }" {
                    span { "Edit" }
                    ul class="menu-dropdown" {
                        li { button x-on:click="focusEditor()" { "Focus Editor" } }
                        li { button x-on:click="saveCurrentNote()" { "Save Note" } }
                    }
                }
                li class="menu-item" tabindex="0"
                    x-on:click="toggleMenu('view')"
                    x-bind:class="{ 'open': openMenu === 'view' }" {
                    span { "View" }
                    ul class="menu-dropdown" {
                        li { a href=(base.url("/")) { "Show All Notes" } }
                        // Pane collapsing is a desktop split-view affordance.
                        // On mobile the tab bar already decides which single
                        // pane is shown, so these are hidden there; see
                        // `.desktop-only` in style.css.
                        li class="desktop-only" { hr; }
                        li class="desktop-only" {
                            button x-on:click="toggleEditor()"
                                x-bind:class="{ 'active-view': !editorVisible }"
                                x-text="editorVisible ? 'Hide Editor' : 'Show Editor'" {
                                "Toggle Editor"
                            }
                        }
                        li class="desktop-only" {
                            button x-on:click="togglePreview()"
                                x-bind:class="{ 'active-view': !previewVisible }"
                                x-text="previewVisible ? 'Hide Preview' : 'Show Preview'" {
                                "Toggle Preview"
                            }
                        }
                    }
                }
                li class="menu-item" tabindex="0"
                    x-on:click="toggleMenu('theme')"
                    x-bind:class="{ 'open': openMenu === 'theme' }" {
                    span { "Theme" }
                    ul class="menu-dropdown" {
                        @for theme in PREDEFINED_THEMES {
                            li {
                                form action=(base.url("/theme")) method="post" {
                                    input type="hidden" name="theme" value=(theme.name);
                                    button type="submit" { (theme.name) }
                                }
                            }
                        }
                    }
                }
                li class="menu-item" tabindex="0"
                    x-on:click="toggleMenu('help')"
                    x-bind:class="{ 'open': openMenu === 'help' }" {
                    span { "Help" }
                    ul class="menu-dropdown" {
                        li { a href=(base.url("/about")) { "About EternaLibre Notes" } }
                        li {
                            a href="https://www.gnu.org/licenses/gpl-3.0.html"
                                target="_blank"
                                rel="noopener noreferrer" { "GPL v3 License" }
                        }
                    }
                }
            }
            // From the manifest, so the title bar and the About page can
            // never disagree. This was a hardcoded "v0.1" that had already
            // drifted from the crate's 0.1.0.
            // Don't hardcode versions like a dummy!
            span class="app-title" {
                (format!("EternaLibre Notes v{}", env!("CARGO_PKG_VERSION")))
            }
        }
    }
}

fn render_toolbar(
    current_note: Option<&Note>,
    query: Option<&str>,
    theme_name: &str,
    base: &Base<'_>,
) -> Markup {
    html! {
        div class="toolbar" {
            div class="toolbar-group" {
                form action=(base.url("/notes")) method="post" {
                    button type="submit" class="btn btn-primary" { "+ New Note" }
                }
                @if let Some(note) = current_note {
                    form action=(base.url(&format!("/notes/{}", note.id))) method="post" {
                        input type="hidden" name="_method" value="delete";
                        button type="submit" class="btn btn-danger" { "Delete" }
                    }
                }
            }
            div class="toolbar-group" {
                form action=(base.url("/")) method="get" class="search-form" {
                    input
                        type="search"
                        name="q"
                        placeholder="Search..."
                        value=[query]
                        x-on:input="filterNotes($event.target.value)";
                }
            }
            div class="toolbar-group" {
                form action=(base.url("/theme")) method="post" class="theme-form" x-on:change="$event.target.form.submit()" {
                    label { "Theme:" }
                    select name="theme" {
                        @for theme in PREDEFINED_THEMES {
                            option value=(theme.name) selected[(theme.name == theme_name)] { (theme.name) }
                        }
                    }
                }
            }
        }
    }
}

fn render_sidebar(
    notes: &[Note],
    current_note: Option<&Note>,
    default_tab: &str,
    base: &Base<'_>,
) -> Markup {
    let active = if default_tab == "notes" {
        " mobile-active"
    } else {
        ""
    };
    html! {
        aside class={"sidebar" (active)} data-tab-pane="notes"
            x-bind:class="{ 'mobile-active': tab === 'notes' }" {
            div class="sidebar-header" { "Notes" }
            ul class="note-list" {
                @if notes.is_empty() {
                    li class="note-item empty" { "No notes yet." }
                } @else {
                    @for note in notes {
                        li class={"note-item " (if current_note.map(|n| n.id == note.id).unwrap_or(false) { "active" } else { "" })}
                            data-title=(note.title.to_lowercase())
                            data-content=(note.content.to_lowercase()) {
                            a href=(base.url(&format!("/notes/{}", note.id))) {
                                div class="note-title" { (note.title) }
                                div class="note-meta" { (relative_time(note.updated_at)) }
                                div class="note-preview" { (note.preview(80)) }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn render_editor(current_note: Option<&Note>, default_tab: &str, base: &Base<'_>) -> Markup {
    let active = if default_tab == "write" {
        " mobile-active"
    } else {
        ""
    };
    html! {
        section class={"editor-pane" (active)} data-tab-pane="write"
            x-data="{ saving: false }"
            x-bind:class="{ 'mobile-active': tab === 'write' }" {
            @if let Some(note) = current_note {
                form
                    id="note-form"
                    action=(base.url(&format!("/notes/{}", note.id)))
                    method="post"
                    x-on:submit="saving = true"
                    class="note-form" {
                    input type="hidden" name="_method" value="put";
                    input
                        type="text"
                        name="title"
                        class="note-title-input"
                        placeholder="Note title"
                        value=(note.title);
                    textarea
                        id="editor"
                        name="content"
                        class="note-content-input"
                        placeholder="Write Markdown here..."
                        spellcheck="false"
                        x-on:input="updatePreview($event.target.value)"
                        x-ref="editor" { (note.content) }
                    div class="form-actions" {
                        button type="submit" class="btn btn-primary" x-bind:disabled="saving" {
                            span x-show="!saving" { "Save" }
                            span x-show="saving" { "Saving..." }
                        }
                    }
                }
            } @else {
                div class="empty-state" {
                    p { "Select a note or create a new one." }
                }
            }
        }
    }
}

fn render_preview(html_content: &str, default_tab: &str) -> Markup {
    let active = if default_tab == "preview" {
        " mobile-active"
    } else {
        ""
    };
    html! {
        section class={"preview-pane" (active)} data-tab-pane="preview"
            x-bind:class="{ 'mobile-active': tab === 'preview' }" {
            div id="preview" class="markdown-body" x-html="previewHtml" {
                (PreEscaped(html_content))
            }
        }
    }
}

fn render_status_bar(note_count: usize, theme_name: &str) -> Markup {
    html! {
        footer class="status-bar" {
            span { (note_count) " Notes" }
            span { "Saved" }
            span { "UTF-8" }
            span { "Markdown" }
            span { "Theme: " (theme_name) }
        }
    }
}

pub fn render_preview_fragment(html_content: &str) -> Markup {
    html! {
        div class="markdown-body" { (PreEscaped(html_content)) }
    }
}

/// Renders the `/login` page.
///
/// TOTP enrolment lives in the terminal (`setup::run_interactive_setup`) and
/// completes before the listener binds, so there is deliberately no setup
/// variant of this page: an HTTP-reachable enrolment endpoint would let
/// anyone who can reach the port before the operator finish claim the
/// device. `error` is displayed inline above the form.
pub fn render_auth_page(theme_name: &str, error: Option<&str>, base: &Base<'_>) -> Markup {
    let theme = get_theme(theme_name);
    html! {
        (DOCTYPE)
        html lang="en" data-theme=(theme_name) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Unlock - EternaLibre Notes" }
                style { (PreEscaped(theme.to_css_block())) }
                style { (PreEscaped(include_str!("../static/style.css"))) }
            }
            body class="auth-page" {
                main class="auth-card" {
                    h1 { "EternaLibre Notes" }
                    p class="auth-subtitle" {
                        "Enter the 6-digit code from your authenticator app."
                    }

                    @if let Some(err) = error {
                        div class="auth-error" role="alert" { (err) }
                    }

                    form method="post" action=(base.url("/login")) class="auth-form" {
                        label for="code" { "6-digit code" }
                        input
                            type="text"
                            id="code"
                            name="code"
                            class="auth-input"
                            placeholder="000000"
                            inputmode="numeric"
                            autocomplete="one-time-code"
                            pattern="[0-9]{6}"
                            maxlength="6"
                            required
                            autofocus
                            spellcheck="false";
                        button type="submit" class="btn btn-primary" { "Unlock" }
                    }

                    p class="auth-footer" { "EternaLibre Notes" }
                }
            }
        }
    }
}

pub fn render_about(theme_name: &str, base: &Base<'_>) -> Markup {
    let theme = get_theme(theme_name);
    let themes = crate::themes::PREDEFINED_THEMES;
    let dark_themes = themes.iter().filter(|t| t.is_dark()).count();
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                // Without this the browser falls back to a ~980px layout
                // viewport and scales the page down, so every
                // `@media (max-width: 600px)` rule silently stops matching
                // and the desktop layout is what you get. This was the only
                // screen in the app missing the tag.
                meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover";
                title { "About - EternaLibre Notes" }
                style { (PreEscaped(theme.to_css_block())) }
                style { (PreEscaped(include_str!("../static/style.css"))) }
            }
            body class="about-page" {
                (render_menu_bar(base))
                main {
                    h1 { "EternaLibre Notes" }
                    p class="tagline" { "Free notes, forever." }
                    p {
                        "A server-side rendered, copyleft Markdown notes application written in Rust. \
                         A toolkit for making a private notes cloud: run the server yourself, \
                         on your own hardware, and the notes go nowhere else."
                    }
                    p {
                        "The server binds to all interfaces by default, so you can reach your notes \
                         from a phone or another machine on your network. It is protected by a TOTP \
                         code from your authenticator app, and it never talks to a third party."
                    }
                    ul class="about-features" {
                        // Counted from the palette itself, and split by
                        // background luminance. This number used to be
                        // hardcoded and had drifted to "28" for what was
                        // really 25 themes.
                        li {
                            (format!(
                                "{} built-in themes ({} dark, {} light) with a full \
                                 ANSI palette — Solarized, Dracula, Nord, Monokai, and more",
                                themes.len(),
                                dark_themes,
                                themes.len() - dark_themes,
                            ))
                        }
                        li { "Syntax highlighting for fenced code blocks" }
                        li { "Notes stored as plain Markdown files on disk" }
                        li { "TOTP-locked, with no accounts or third-party services" }
                        li { "Note editing, search, and theming work without JavaScript" }
                    }
                    h2 { "Author" }
                    ul class="about-links" {
                        li {
                            a href="https://github.com/MARKMENTAL" rel="noopener noreferrer" {
                                "GitHub — MARKMENTAL"
                            }
                        }
                        li {
                            a href="https://mentalnet.xyz/forgejo-v2/MARKMENTAL" rel="noopener noreferrer" {
                                "Forgejo — mentalnet.xyz"
                            }
                        }
                    }
                    h2 { "Other projects" }
                    ul class="about-links" {
                        li {
                            a href="https://github.com/MARKMENTAL/tuxdock"
                                rel="noopener noreferrer" {
                                "tux-dock — a C++ TUI for managing Docker containers"
                            }
                        }
                        li {
                            a href="https://github.com/MARKMENTAL/mentalnet-gnu-linux"
                                rel="noopener noreferrer" {
                                "mentalnet GNU/Linux — a TTY-only distribution for vintage \
                                 Pentium hardware"
                            }
                        }
                    }
                    p class="about-colophon" {
                        (format!(
                            "Version {}. Licensed under the GNU General Public \
                             License v3.0 or later. No telemetry, no accounts, no vendor.",
                            env!("CARGO_PKG_VERSION"),
                        ))
                    }
                    a class="back-link" href=(base.url("/")) { "Back to notes" }
                }
                (render_status_bar(0, theme_name))
            }
        }
    }
}

pub fn relative_time(dt: DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(dt);

    if diff.num_seconds() < 60 {
        "just now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else if diff.num_days() < 30 {
        format!("{}d ago", diff.num_days())
    } else {
        dt.format("%b %d, %Y").to_string()
    }
}
