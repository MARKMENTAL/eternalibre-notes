use crate::markdown::render_markdown;
use crate::notes::Note;
use crate::themes::{get_theme, PREDEFINED_THEMES};
use chrono::{DateTime, Utc};
use maud::{html, Markup, PreEscaped, DOCTYPE};

pub fn render_app(
    theme_name: &str,
    notes: &[Note],
    current_note: Option<&Note>,
    query: Option<&str>,
    message: Option<&str>,
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
                title { "Rasuti Notes" }
                style { (PreEscaped(css)) }
                style { (PreEscaped(include_str!("../static/style.css"))) }
                script defer src="/static/alpine.min.js" {}
            }
            body x-data="app()" x-on:keydown="handleKeydown($event)" {
                (render_menu_bar())
                (render_toolbar(current_note, query, theme_name))
                @if let Some(msg) = message {
                    div class="flash-message" { (msg) }
                }
                (render_tab_bar(default_tab))
                main class="app-body" {
                    (render_sidebar(notes, current_note, default_tab))
                    (render_editor(current_note, default_tab))
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

fn render_menu_bar() -> Markup {
    html! {
        nav class="menu-bar" {
            ul class="menu-list" {
                li class="menu-item" tabindex="0"
                    x-on:click="toggleMenu('file')"
                    x-bind:class="{ 'open': openMenu === 'file' }" {
                    span { "File" }
                    ul class="menu-dropdown" {
                        li {
                            form action="/notes" method="post" {
                                button type="submit" { "New Note" }
                            }
                        }
                        li { hr; }
                        li {
                            a href="/export" { "Export All Notes" }
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
                        li { a href="/" { "Show All Notes" } }
                        li { button x-on:click="togglePreview()" { "Toggle Preview" } }
                    }
                }
                li class="menu-item" tabindex="0"
                    x-on:click="toggleMenu('theme')"
                    x-bind:class="{ 'open': openMenu === 'theme' }" {
                    span { "Theme" }
                    ul class="menu-dropdown" {
                        @for theme in PREDEFINED_THEMES {
                            li {
                                form action="/theme" method="post" {
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
                        li { a href="/about" { "About Rasuti Notes" } }
                        li { a href="https://www.gnu.org/licenses/gpl-3.0.html" target="_blank" { "GPL v3 License" } }
                    }
                }
            }
            span class="app-title" { "Rasuti Notes ラスティ v0.1" }
        }
    }
}

fn render_toolbar(current_note: Option<&Note>, query: Option<&str>, theme_name: &str) -> Markup {
    html! {
        div class="toolbar" {
            div class="toolbar-group" {
                form action="/notes" method="post" {
                    button type="submit" class="btn btn-primary" { "+ New Note" }
                }
                @if let Some(note) = current_note {
                    form action=(format!("/notes/{}", note.id)) method="post" {
                        input type="hidden" name="_method" value="delete";
                        button type="submit" class="btn btn-danger" { "Delete" }
                    }
                }
            }
            div class="toolbar-group" {
                form action="/" method="get" class="search-form" {
                    input
                        type="search"
                        name="q"
                        placeholder="Search..."
                        value=[query]
                        x-on:input="filterNotes($event.target.value)";
                }
            }
            div class="toolbar-group" {
                form action="/theme" method="post" class="theme-form" x-on:change="$event.target.form.submit()" {
                    label { "Theme:" }
                    select name="theme" {
                        @for theme in PREDEFINED_THEMES {
                            option value=(theme.name) selected[(theme.name == theme_name)] { (theme.name) }
                        }
                    }
                }
                button type="button" class="btn" x-on:click="togglePreview()" { "Preview" }
            }
        }
    }
}

fn render_sidebar(notes: &[Note], current_note: Option<&Note>, default_tab: &str) -> Markup {
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
                            a href=(format!("/notes/{}", note.id)) {
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

fn render_editor(current_note: Option<&Note>, default_tab: &str) -> Markup {
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
                    action=(format!("/notes/{}", note.id))
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
            x-bind:class="{ 'hidden': !previewVisible, 'mobile-active': tab === 'preview' }" {
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

pub fn render_about(theme_name: &str) -> Markup {
    let theme = get_theme(theme_name);
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                title { "About - Rasuti Notes" }
                style { (PreEscaped(theme.to_css_block())) }
                style { (PreEscaped(include_str!("../static/style.css"))) }
            }
            body class="about-page" {
                (render_menu_bar())
                main {
                    h1 { "Rasuti Notes" }
                    p { "A server-side rendered Markdown notes application written in Rust." }
                    p { "Licensed under the GNU General Public License v3.0 or later." }
                    a href="/" { "Back to notes" }
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
