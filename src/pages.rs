use crate::markdown::render_markdown;
use crate::notes::Note;
use crate::themes::{get_theme, PREDEFINED_THEMES};
use chrono::{DateTime, Utc};
use maud::{html, Markup, PreEscaped, DOCTYPE};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

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

    /// The raw prefix, for handing to client-side JS.
    ///
    /// `url()` covers every URL in the rendered markup. This exists for the
    /// one `fetch()` in `app.js`, which has to build the URL client-side and
    /// would otherwise resolve `/preview` against the origin root — behind a
    /// reverse proxy mounted at a subdirectory that misses the app entirely
    /// and returns the proxy's own 404.
    pub fn prefix(&self) -> &str {
        self.0
    }
}

/// Folder state passed to the app template.
pub struct FolderContext<'a> {
    /// All existing folder names, sorted.
    pub folders: &'a [String],
    /// The folder currently being viewed, if any.
    pub active_folder: Option<&'a str>,
}

pub fn render_app(
    theme_name: &str,
    notes: &[Note],
    folder_ctx: &FolderContext<'_>,
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
                (render_menu_bar(base, current_note, folder_ctx.folders))
                (render_toolbar(
                    current_note,
                    query,
                    theme_name,
                    folder_ctx.active_folder,
                    base,
                ))
                @if let Some(msg) = message {
                    div class="flash-message" { "Error: " (msg) }
                }
                (render_tab_bar(default_tab))
                main class="app-body"
                    x-bind:class="{ 'hide-editor': !editorVisible, 'hide-preview': !previewVisible }" {
                    (render_sidebar(
                        notes,
                        folder_ctx.folders,
                        folder_ctx.active_folder,
                        current_note,
                        default_tab,
                        base,
                    ))
                    (render_editor(current_note, folder_ctx.folders, default_tab, base))
                    (render_preview(&rendered_preview, default_tab))
                }
                (render_status_bar(notes.len(), theme_name))
                script {
                    "window.__INITIAL_PREVIEW__ = `" (PreEscaped(escape_js(&rendered_preview))) "`;"
                    "window.__BASE_PATH__ = `" (PreEscaped(escape_js(base.prefix()))) "`;"
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

fn render_menu_bar(base: &Base<'_>, current_note: Option<&Note>, folders: &[String]) -> Markup {
    html! {
        nav class="menu-bar" {
            ul class="menu-list" {
                li class="menu-item" tabindex="0"
                    x-on:click="toggleMenu('file')"
                    x-bind:class="{ 'open': openMenu === 'file' }" {
                    span { "File" }
                    ul class="menu-dropdown file-menu-dropdown" {
                        li {
                            form action=(base.url("/notes")) method="post" {
                                button type="submit" { "New Note" }
                            }
                        }
                        li { hr; }
                        li {
                            a href=(base.url("/export")) { "Export All Notes" }
                        }
                        li class="menu-submenu" {
                            @if let Some(note) = current_note {
                                button type="button" class="menu-submenu-trigger"
                                    x-on:click="toggleSubmenu('noteExport', $event)"
                                    x-bind:aria-expanded="openSubmenu === 'noteExport'" {
                                    "Export Note As…"
                                    span class="submenu-chevron" aria-hidden="true" { "›" }
                                }
                                ul class="menu-submenu-list"
                                    x-bind:class="{ 'open': openSubmenu === 'noteExport' }" {
                                    li {
                                        a href=(base.url(&format!("/notes/{}/export/pdf", note.id))) {
                                            "Print/PDF"
                                        }
                                    }
                                    li {
                                        a href=(base.url(&format!("/notes/{}/export/markdown", note.id))) {
                                            "Markdown"
                                        }
                                    }
                                    li {
                                        a href=(base.url(&format!("/notes/{}/export/html", note.id))) {
                                            "HTML"
                                        }
                                    }
                                }
                            } @else {
                                button type="button" class="menu-submenu-trigger" disabled
                                    aria-disabled="true" { "Export Note As…" }
                            }
                        }
                        // Available at every width, including mobile. The
                        // editor pane's `.move-folder-bar` is the other mobile
                        // entry point and is `display: none` above 600px; both
                        // post to the same route.
                        li class="menu-submenu" {
                            @if let Some(note) = current_note {
                                button type="button" class="menu-submenu-trigger"
                                    x-on:click="toggleSubmenu('noteMove', $event)"
                                    x-bind:aria-expanded="openSubmenu === 'noteMove'" {
                                    "Move to Folder…"
                                    span class="submenu-chevron" aria-hidden="true" { "›" }
                                }
                                ul class="menu-submenu-list"
                                    x-bind:class="{ 'open': openSubmenu === 'noteMove' }" {
                                    // One plain form per destination, so a
                                    // click is a real POST to /notes/:id/move
                                    // and no JavaScript runs the move. Matches
                                    // how the Theme menu works.
                                    li {
                                        form
                                            action=(base.url(&format!("/notes/{}/move", note.id)))
                                            method="post" {
                                            input type="hidden" name="folder" value="";
                                            button type="submit" { "(Unfiled)" }
                                        }
                                    }
                                    @for folder in folders {
                                        li {
                                            form
                                                action=(base.url(&format!(
                                                    "/notes/{}/move",
                                                    note.id
                                                )))
                                                method="post" {
                                                input type="hidden" name="folder" value=(folder);
                                                // `title` because folder names run to 100
                                                // characters and the flyout is capped
                                                // to the viewport width, so a long name
                                                // can wrap to several lines.
                                                button type="submit" title=(folder) { (folder) }
                                            }
                                        }
                                    }
                                }
                            } @else {
                                button type="button" class="menu-submenu-trigger" disabled
                                    aria-disabled="true" { "Move to Folder…" }
                            }
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
    active_folder: Option<&str>,
    base: &Base<'_>,
) -> Markup {
    html! {
        div class="toolbar" {
            div class="toolbar-group" {
                form action=(base.url("/notes")) method="post" {
                    @if let Some(folder) = active_folder {
                        input type="hidden" name="folder" value=(folder);
                    }
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

/// Whether a folder nav item should carry a count badge.
///
/// `this_folder` is `None` for the "All Notes" item, which is itself scoped to
/// the root rather than to any one folder.
///
/// Badges are a whole-sidebar summary, which is only meaningful in the All
/// Notes view where every folder is visible at once. Inside a folder the
/// sidebar lists just that folder's notes, so a count beside any *other*
/// folder would be zero — and zero is worse than absent, because it reads as
/// "this folder is empty" rather than "you are not looking at it". So a folder
/// view shows exactly one badge, on the folder you are in.
fn show_nav_count(active_folder: Option<&str>, this_folder: Option<&str>) -> bool {
    match active_folder {
        None => true,
        Some(active) => Some(active) == this_folder,
    }
}

fn render_sidebar(
    notes: &[Note],
    folders: &[String],
    active_folder: Option<&str>,
    current_note: Option<&Note>,
    default_tab: &str,
    base: &Base<'_>,
) -> Markup {
    let active = if default_tab == "notes" {
        " mobile-active"
    } else {
        ""
    };

    // Group notes by folder for the "All Notes" view.
    let mut unfiled: Vec<&Note> = Vec::new();
    let mut by_folder: std::collections::BTreeMap<String, Vec<&Note>> =
        std::collections::BTreeMap::new();
    for note in notes {
        match &note.folder {
            Some(f) => by_folder.entry(f.clone()).or_default().push(note),
            None => unfiled.push(note),
        }
    }

    html! {
        aside class={"sidebar" (active)} data-tab-pane="notes"
            x-bind:class="{ 'mobile-active': tab === 'notes' }" {
            div class="sidebar-header" { "Notes" }
            form action=(base.url("/folders")) method="post" class="new-folder-form" {
                input type="text" name="name" placeholder="New folder name...";
                button type="submit" { "+ New Folder" }
            }
            ul class="folder-nav" {
                // An empty drop folder is the root: dropping a note on "All
                // Notes" un-files it, which is the only way back out of a
                // folder once every note has left it.
                li class={"nav-item" (if active_folder.is_none() { " active" } else { "" })}
                    data-drop-folder="" {
                    a href=(base.url("/")) {
                        span class="nav-label" { "All Notes" }
                        @if show_nav_count(active_folder, None) {
                            span class="nav-count" { (notes.len()) }
                        }
                    }
                }
                @for folder in folders {
                    li class={"nav-item" (if active_folder == Some(folder.as_str()) { " active" } else { "" })}
                        data-drop-folder=(folder) {
                        a href=(folder_url(base, folder)) {
                            span class="nav-label" { (folder) }
                            @if show_nav_count(active_folder, Some(folder)) {
                                span class="nav-count" { (by_folder.get(folder).map(|v| v.len()).unwrap_or(0)) }
                            }
                        }
                    }
                }
            }
            ul class="note-list" {
                @if notes.is_empty() {
                    li class="note-item empty" { "No notes yet." }
                } @else if active_folder.is_none() {
                    // Grouped view: unfiled first, then folders alphabetically.
                    @if !unfiled.is_empty() {
                        li class="folder-header" { "Unfiled" }
                        @for note in &unfiled {
                            (render_note_item(note, current_note, base))
                        }
                    }
                    @for (folder, folder_notes) in &by_folder {
                        li class="folder-header" { (folder) }
                        @for note in folder_notes {
                            (render_note_item(note, current_note, base))
                        }
                    }
                } @else {
                    // Flat view of the active folder.
                    @for note in notes {
                        (render_note_item(note, current_note, base))
                    }
                }
            }
        }
    }
}

fn render_note_item(note: &Note, current_note: Option<&Note>, base: &Base<'_>) -> Markup {
    let is_active = current_note.map(|n| n.id == note.id).unwrap_or(false);
    html! {
        li class={"note-item " (if is_active { "active" } else { "" })}
            draggable="true"
            data-note-id=(note.id)
            data-title=(note.title.to_lowercase())
            data-content=(note.content.to_lowercase()) {
            // `draggable="false"` matters: anchors are natively draggable, so
            // without this the browser starts a link drag (URL ghost, no
            // dataTransfer payload) and no folder ever lights up.
            a href=(base.url(&format!("/notes/{}", note.id))) draggable="false" {
                div class="note-title" { (note.title) }
                div class="note-meta" { (relative_time(note.updated_at)) }
                div class="note-preview" { (note.preview(80)) }
            }
        }
    }
}

/// Builds a URL that filters the sidebar to a single folder.
fn folder_url(base: &Base<'_>, folder: &str) -> String {
    let encoded = utf8_percent_encode(folder, NON_ALPHANUMERIC).to_string();
    base.url(&format!("/?folder={encoded}"))
}

/// DOM id for a note's move-folder select, so the `<label for=…>` can point at
/// it rather than the select relying on `aria-label` alone.
///
/// Safe as an element id because it is derived from a UUID.
fn move_select_id(note: &Note) -> String {
    format!("move-folder-{}", note.id)
}

fn render_editor(
    current_note: Option<&Note>,
    folders: &[String],
    default_tab: &str,
    base: &Base<'_>,
) -> Markup {
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
                // A sibling of `#note-form`, never a child of it: nesting a
                // form inside a form is invalid HTML and browsers drop the
                // inner element outright, which would silently delete this
                // control without any error to notice.
                //
                // The mobile entry point for re-filing. Do NOT add
                // `desktop-only` here: that class means "hide at <=600px", so
                // pairing it with the `min-width: 601px` rule in style.css
                // that hides this bar on desktop hides it at *every* width.
                // Desktop gets File > Move to Folder… instead. Being a plain
                // form, this one also works with JavaScript off, which the
                // menu cannot.
                div class="move-folder-bar" {
                    div class="note-folder" {
                        @if let Some(folder) = &note.folder {
                            "Folder: " (folder)
                        } @else {
                            "Unfiled"
                        }
                    }
                    // No `x-on:change`. A select cannot submit a form on its
                    // own, so an auto-submit handler would leave this control
                    // inert without JS — and working without JS is the entire
                    // reason the bar exists alongside the menu. Select picks
                    // the folder, the button commits it, one path either way.
                    form
                        class="move-folder-form"
                        action=(base.url(&format!("/notes/{}/move", note.id)))
                        method="post" {
                        label class="move-label" for=(move_select_id(note)) { "Move to" }
                        select id=(move_select_id(note)) name="folder" {
                            option value="" selected[(note.folder.is_none())] { "Unfiled" }
                            @for folder in folders {
                                option
                                    value=(folder)
                                    selected[(note.folder.as_deref() == Some(folder.as_str()))] {
                                    (folder)
                                }
                            }
                        }
                        button type="submit" class="btn" { "Move" }
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

/// Renders a self-contained, theme-aware note document for HTML download or
/// the browser's print-to-PDF flow.
pub fn render_note_export(
    note: &Note,
    theme: &crate::themes::Theme,
    print_on_load: bool,
) -> Markup {
    let rendered = render_markdown(&note.content, theme);
    html! {
        (DOCTYPE)
        html lang="en" data-theme=(theme.name) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (note.title) }
                style { (PreEscaped(theme.to_css_block())) }
                style { (PreEscaped(include_str!("../static/style.css"))) }
                style {
                    (PreEscaped(r#"
                        html, body { height: auto; min-height: 100%; }
                        body { display: block; height: auto; min-height: 100vh; overflow: auto; }
                        .note-export { max-width: 900px; margin: 0 auto; padding: 48px; }
                        .note-export-title { margin: 0 0 28px; overflow-wrap: anywhere; }
                        @media print {
                          @page { margin: 18mm; }
                          html, body {
                            height: auto;
                            min-height: 0;
                            background-color: var(--app-bg) !important;
                            color: var(--app-fg) !important;
                            print-color-adjust: exact;
                            -webkit-print-color-adjust: exact;
                          }
                          body { display: block; overflow: visible; }
                          .note-export { max-width: none; margin: 0; padding: 0; }
                          .note-export-title { break-after: avoid-page; }
                          .markdown-body pre {
                            overflow: visible;
                            white-space: pre-wrap;
                            overflow-wrap: anywhere;
                          }
                          .markdown-body pre, .markdown-body blockquote,
                          .markdown-body table { break-inside: avoid; }
                          .markdown-body a { color: inherit; text-decoration: none; }
                        }
                    "#))
                }
            }
            body class="note-export-page" {
                main class="note-export" {
                    h1 class="note-export-title" { (note.title) }
                    div class="markdown-body" { (PreEscaped(rendered)) }
                }
                @if print_on_load {
                    script { "window.addEventListener('load', () => window.print());" }
                }
            }
        }
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
                // The About page has no notes pane and no selected note, so
                // there is nothing to move; an empty folder list renders the
                // submenu trigger disabled.
                (render_menu_bar(base, None, &[]))
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
                        // really 24 themes.
                        li {
                            (format!(
                                "{} built-in themes ({} dark, {} light) with a full \
                                 ANSI palette — Solarized, Dracula, KDE Breeze, Gentoo, and more",
                                themes.len(),
                                dark_themes,
                                themes.len() - dark_themes,
                            ))
                        }
                        li { "Syntax highlighting for fenced code blocks" }
                        li { "Notes stored as plain Markdown files on disk" }
                        li { "TOTP-locked, with no accounts or third-party services" }
                        li { "Note editing, search, and theming work without JavaScript" }
                        li { "Export saved notes as Markdown, themed HTML, or PDF via the browser print dialog" }
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

#[cfg(test)]
mod tests {
    use super::show_nav_count;

    #[test]
    fn all_notes_view_badges_every_item() {
        // No folder selected: the sidebar shows every folder at once, so the
        // counts are a real summary.
        assert!(show_nav_count(None, None), "All Notes");
        assert!(show_nav_count(None, Some("Work")), "a folder");
        assert!(
            show_nav_count(None, Some("Anything else")),
            "another folder"
        );
    }

    #[test]
    fn folder_view_badges_only_the_selected_folder() {
        assert!(
            show_nav_count(Some("Work"), Some("Work")),
            "the open folder"
        );
        assert!(
            !show_nav_count(Some("Work"), Some("Home")),
            "a sibling folder"
        );
        assert!(
            !show_nav_count(Some("Work"), None),
            "All Notes: the badge there would be this folder's count under a \
             different label"
        );
    }

    /// Guards the direction of the comparison. Matching on "is this folder the
    /// active one" the wrong way round — inverting it by accident, or by
    /// comparing against the wrong argument — would badge every folder
    /// *except* the open one, which looks like a working feature until you
    /// notice the open folder is the one missing its count.
    #[test]
    fn badge_follows_the_pointer_not_the_argument_order() {
        // Two different folders: exactly one may be badged, and it must be the
        // active one.
        let active = "Work";
        let other = "Home";
        assert!(show_nav_count(Some(active), Some(active)));
        assert!(!show_nav_count(Some(active), Some(other)));
    }
}
