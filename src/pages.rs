use crate::markdown::render_markdown;
use crate::notes::Note;
use crate::quotes::Quote;
use crate::themes::{get_theme, PREDEFINED_THEMES};
use crate::weather::{TempUnit, WeatherDisplay};
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

/// Per-request data that varies with the current app page.
pub struct AppPageContext<'a> {
    pub current_note: Option<&'a Note>,
    pub recent_notes: &'a [Note],
    pub weather: &'a WeatherDisplay,
    pub quotes: &'a [Quote],
    pub query: Option<&'a str>,
    pub message: Option<&'a str>,
}

pub fn render_app(
    theme_name: &str,
    notes: &[Note],
    folder_ctx: &FolderContext<'_>,
    page: &AppPageContext<'_>,
    base: &Base<'_>,
) -> Markup {
    let theme = get_theme(theme_name);
    let css = theme.to_css_block();
    let current_note = page.current_note;
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
    // The welcome state has nothing to preview, so the preview pane is not
    // rendered at all and the layout drops to two columns.
    let has_note = current_note.is_some();
    let body_class = if has_note { "" } else { " no-preview" };

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
                    page.query,
                    theme_name,
                    folder_ctx.active_folder,
                    base,
                ))
                @if let Some(msg) = page.message {
                    div class="flash-message" { "Error: " (msg) }
                }
                (render_tab_bar(default_tab, has_note))
                main class={"app-body" (body_class)}
                    x-bind:class="{ 'hide-editor': !editorVisible, 'hide-preview': !previewVisible }" {
                    (render_sidebar(
                        notes,
                        folder_ctx.folders,
                        folder_ctx.active_folder,
                        current_note,
                        default_tab,
                        base,
                    ))
                (render_editor(
                    current_note,
                    folder_ctx.folders,
                    page.recent_notes,
                    page.weather,
                    page.quotes,
                    default_tab,
                    base,
                ))
                    (render_preview(&rendered_preview, default_tab, has_note))
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

fn render_tab_bar(default_tab: &str, has_note: bool) -> Markup {
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
            @if has_note {
                button type="button" class="tab-button" data-tab-target="preview"
                    x-on:click="setTab('preview')"
                    x-bind:class="{ 'active': tab === 'preview' }" {
                    span class="tab-icon" { "\u{1F4C4}" }
                    span { "Preview" }
                }
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
    let theme_return = theme_return_path(current_note);
    let weather_href = dedicated_page_href(base, "/weather", current_note);
    let welcome_href = dedicated_page_href(base, "/welcome", current_note);
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
                        li { button x-on:click="saveCurrentNote()" { "Save Note" } }
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
                        // No preview pane exists without an open note, so the
                        // toggle would only offer an empty view.
                        @if current_note.is_some() {
                            li class="desktop-only" {
                                button x-on:click="togglePreview()"
                                    x-bind:class="{ 'active-view': !previewVisible }"
                                    x-text="previewVisible ? 'Hide Preview' : 'Show Preview'" {
                                    "Toggle Preview"
                                }
                            }
                        }
                        // Available at every width. The folder nav is a sidebar
                        // sub-section, not a top-level pane, so collapsing it is
                        // useful on mobile too — the pull-tab button is the other
                        // entry point and shares this same state.
                        li {
                            button x-on:click="toggleFolderNav()"
                                x-bind:class="{ 'active-view': folderNavCollapsed }"
                                x-text="folderNavCollapsed ? 'Show Folder Nav' : 'Hide Folder Nav'" {
                                "Toggle Folder Nav"
                            }
                        }
                        // Mobile-only shortcuts to the dedicated pages. Desktop
                        // already shows the full dashboard, so these entries
                        // stay out of the way there; see `.mobile-only`.
                        li class="mobile-only" {
                            a href=(welcome_href) { "Welcome" }
                        }
                        li class="mobile-only" {
                            a href=(weather_href) { "Weather" }
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
                                    input type="hidden" name="return_to" value=(theme_return);
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
    let theme_return = theme_return_path(current_note);
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
                    form
                        action=(base.url(&format!("/notes/{}", note.id)))
                        method="post"
                        data-delete-label=(format!("[{}-{}]",
                            note.folder.as_deref().unwrap_or("Unfiled"),
                            note.title))
                        onsubmit="return confirm('Are you sure you want to delete ' + this.dataset.deleteLabel + '? This cannot be undone.')" {
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
                    input type="hidden" name="return_to" value=(theme_return);
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
            div class="folder-nav-container"
                x-bind:class="{ 'collapsed': folderNavCollapsed }" {
                button type="button" class="folder-nav-toggle"
                    x-on:click="toggleFolderNav()"
                    x-bind:aria-expanded="(!folderNavCollapsed).toString()"
                    aria-controls="folder-nav-list"
                    aria-label="Toggle folder navigation" {
                    span class="folder-nav-toggle-icon" aria-hidden="true" { "▾" }
                }
                ul id="folder-nav-list" class="folder-nav" {
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
    recent_notes: &[Note],
    weather: &WeatherDisplay,
    quotes: &[Quote],
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
                (render_welcome(recent_notes, weather, quotes, base))
            }
        }
    }
}

fn theme_return_path(current_note: Option<&Note>) -> String {
    current_note
        .map(|note| format!("/notes/{}", note.id))
        .unwrap_or_else(|| "/".to_string())
}

fn dedicated_page_href(base: &Base<'_>, path: &str, current_note: Option<&Note>) -> String {
    let target = theme_return_path(current_note);
    let encoded = utf8_percent_encode(&target, NON_ALPHANUMERIC).to_string();
    base.url(&format!("{path}?return_to={encoded}"))
}

fn render_welcome_heading(quotes: &[Quote]) -> Markup {
    html! {
        header class="welcome-heading" {
            p class="welcome-eyebrow" { "YOUR NOTES, YOUR SPACE" }
            h1 id="welcome-title" { "Welcome to EternaLibre Notes" }
            p class="welcome-tagline" { "A quiet place for ideas, plans, and everything worth keeping." }
            (render_quote_rotator(quotes))
        }
    }
}

fn render_recent_card(recent_notes: &[Note], base: &Base<'_>) -> Markup {
    html! {
        section class="welcome-card recent-card" aria-labelledby="recent-notes-title" {
            h2 id="recent-notes-title" { "Recently updated" }
            @if recent_notes.is_empty() {
                p class="welcome-muted" { "Your notes will show up here. Create one whenever you’re ready." }
            } @else {
                ul class="welcome-recent-list" {
                    @for note in recent_notes.iter().take(5) {
                        li {
                            a href=(base.url(&format!("/notes/{}", note.id))) {
                                span class="welcome-note-title" { (note.title) }
                                span class="welcome-note-meta" { (relative_time(note.updated_at)) }
                                @let preview = note.preview(100);
                                @if !preview.is_empty() {
                                    span class="welcome-note-preview" { (preview) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn render_welcome(
    recent_notes: &[Note],
    weather: &WeatherDisplay,
    quotes: &[Quote],
    base: &Base<'_>,
) -> Markup {
    html! {
        section class="welcome-state" aria-labelledby="welcome-title" {
            (render_welcome_heading(quotes))
            div class="welcome-columns" {
                (render_recent_card(recent_notes, base))
                section class="welcome-card weather-card" aria-labelledby="weather-title" {
                    h2 id="weather-title" { "Local weather" }
                    (render_weather_table(weather))
                    (render_weather_form(weather, base, "weather-location"))
                }
            }
            (render_quotes_manager(quotes, base))
        }
    }
}

/// Rotating quote under the welcome tagline. The first quote is server-rendered
/// so the block reads sensibly with JavaScript disabled; `app.js` swaps in the
/// rest on a timer when more than one quote exists.
fn render_quote_rotator(quotes: &[Quote]) -> Markup {
    html! {
        @if quotes.is_empty() {
        } @else {
            div class="welcome-quotes" {
                blockquote id="welcome-quote" class="welcome-quote" aria-live="polite" {
                    p id="welcome-quote-text" class="welcome-quote-text" { (quotes[0].text) }
                    @if !quotes[0].author.trim().is_empty() {
                        footer {
                            "— " span id="welcome-quote-author" { (quotes[0].author) }
                        }
                    } @else {
                        footer {
                            "— " span id="welcome-quote-author" { "Anonymous" }
                        }
                    }
                }
                @if quotes.len() > 1 {
                    div class="welcome-quote-controls" {
                        button type="button" id="quote-prev" class="btn btn-small" { "‹ Prev" }
                        button type="button" id="quote-pause" class="btn btn-small" aria-pressed="false" { "Pause" }
                        button type="button" id="quote-next" class="btn btn-small" { "Next ›" }
                    }
                }
                script { (PreEscaped(welcome_quotes_js(quotes))) }
            }
        }
    }
}

fn welcome_quotes_js(quotes: &[Quote]) -> String {
    // JSON keeps user-supplied quotes intact; escaping `</` stops a quote from
    // closing the script element itself.
    let json = serde_json::to_string(quotes).unwrap_or_else(|_| "[]".to_string());
    format!(
        "window.__WELCOME_QUOTES__ = {};",
        json.replace("</", "<\\/")
    )
}

fn render_weather_table(weather: &WeatherDisplay) -> Markup {
    html! {
        @if let Some(location) = weather.location.as_deref() {
            @if weather.current.is_none() && weather.days.is_empty() {
                p class="welcome-muted" { "Weather is temporarily unavailable." }
            } @else {
                div class="weather-table-wrapper" {
                    table class="weather-table" {
                        caption class="weather-caption" {
                            "Weather for " (location) " (" (weather.units.label()) ")"
                        }
                        thead {
                            tr {
                                th scope="col" { "Day" }
                                th scope="col" { "Conditions" }
                                th scope="col" { "Temp" }
                                th scope="col" { "High" }
                                th scope="col" { "Low" }
                            }
                        }
                        tbody {
                            @if let Some(now) = weather.current.as_ref() {
                                tr class="weather-now-row" {
                                    th scope="row" { "Now" }
                                    td { (now.desc) }
                                    td { (now.temp(weather.units)) }
                                    td class="weather-na" { "—" }
                                    td class="weather-na" { "—" }
                                }
                            }
                            @for day in &weather.days {
                                tr {
                                    th scope="row" { (day.short_date()) }
                                    td { (day.desc) }
                                    td { (day.avg(weather.units)) }
                                    td { (day.high(weather.units)) }
                                    td { (day.low(weather.units)) }
                                }
                            }
                        }
                    }
                }
                @if let Some(now) = weather.current.as_ref() {
                    p class="weather-details" {
                        "Feels like " (now.feels(weather.units)) " · Humidity " (now.humidity) "%"
                    }
                }
            }
            p class="weather-location" { "Location: " (location) }
        } @else {
            p class="welcome-muted" { "Set a location to see current weather from wttr.in." }
        }
    }
}

fn render_weather_form(weather: &WeatherDisplay, base: &Base<'_>, id: &str) -> Markup {
    let units = weather.units;
    html! {
        form action=(base.url("/settings/weather")) method="post" class="weather-location-form" {
            label for=(id) { "Location" }
            input
                id=(id)
                type="text"
                name="location"
                maxlength="100"
                placeholder="City or place name"
                value=[weather.location.as_deref()];
            fieldset class="weather-units" {
                legend { "Units" }
                label {
                    input
                        type="radio"
                        name="units"
                        value="c"
                        checked[units == TempUnit::Celsius];
                    " Celsius"
                }
                label {
                    input
                        type="radio"
                        name="units"
                        value="f"
                        checked[units == TempUnit::Fahrenheit];
                    " Fahrenheit"
                }
            }
            button type="submit" class="btn" { "Save location" }
            p class="weather-privacy" {
                "The server sends this location to wttr.in to retrieve weather. Leave it blank to clear it."
            }
        }
    }
}

fn render_quotes_manager(quotes: &[Quote], base: &Base<'_>) -> Markup {
    html! {
        section class="welcome-card quotes-card" aria-labelledby="quotes-title" {
            h2 id="quotes-title" { "Quotes" }
            p class="welcome-muted" {
                "Add quotes to rotate beneath the welcome message. One submission adds one quote."
            }
            form action=(base.url("/settings/quotes")) method="post" class="quote-form" {
                label for="quote-text" { "Quote" }
                textarea
                    id="quote-text"
                    name="quote"
                    maxlength="500"
                    rows="3"
                    placeholder="Write something worth re-reading…"
                    required="required" {}
                label for="quote-author" { "Author" }
                input
                    id="quote-author"
                    type="text"
                    name="author"
                    maxlength="100"
                    placeholder="Who said it?";
                button type="submit" class="btn" { "Add quote" }
            }
            @if quotes.is_empty() {
                p class="welcome-muted" { "No quotes yet. The rotator appears once you add one." }
            } @else {
                ul class="quote-list" {
                    @for (index, quote) in quotes.iter().enumerate() {
                        li class="quote-item" {
                            blockquote class="quote-item-body" {
                                p { (quote.text) }
                                @if !quote.author.trim().is_empty() {
                                    footer { "— " (quote.author) }
                                }
                            }
                            form action=(base.url("/settings/quotes/delete")) method="post" class="quote-delete-form" {
                                input type="hidden" name="index" value=(index);
                                button type="submit" class="btn btn-small" { "Remove" }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn render_weather_page(
    theme_name: &str,
    weather: &WeatherDisplay,
    return_to: &str,
    base: &Base<'_>,
) -> Markup {
    let theme = get_theme(theme_name);
    html! {
        (DOCTYPE)
        html lang="en" data-theme=(theme_name) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover";
                title { "Weather - EternaLibre Notes" }
                style { (PreEscaped(theme.to_css_block())) }
                style { (PreEscaped(include_str!("../static/style.css"))) }
                script defer src=(base.url("/static/alpine.min.js")) {}
            }
            body class="weather-page" x-data="app()" x-on:keydown="handleKeydown($event)" {
                (render_menu_bar(base, None, &[]))
                main class="weather-page-main" {
                    p class="welcome-eyebrow" { "MOBILE WEATHER" }
                    h1 { "Weather" }
                    section class="welcome-card weather-card" aria-labelledby="weather-page-title" {
                        h2 id="weather-page-title" { "Local weather" }
                        (render_weather_table(weather))
                        (render_weather_form(weather, base, "weather-page-location"))
                    }
                    a class="back-link" href=(base.url(return_to)) { "Back" }
                }
                (render_status_bar(0, theme_name))
                script {
                    "window.__INITIAL_PREVIEW__ = ``;"
                    "window.__BASE_PATH__ = `" (PreEscaped(escape_js(base.prefix()))) "`;"
                    (PreEscaped(include_str!("../static/app.js")))
                }
            }
        }
    }
}

pub fn render_welcome_page(
    theme_name: &str,
    recent_notes: &[Note],
    quotes: &[Quote],
    return_to: &str,
    base: &Base<'_>,
) -> Markup {
    let theme = get_theme(theme_name);
    html! {
        (DOCTYPE)
        html lang="en" data-theme=(theme_name) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover";
                title { "Welcome - EternaLibre Notes" }
                style { (PreEscaped(theme.to_css_block())) }
                style { (PreEscaped(include_str!("../static/style.css"))) }
                script defer src=(base.url("/static/alpine.min.js")) {}
            }
            body class="welcome-page" x-data="app()" x-on:keydown="handleKeydown($event)" {
                (render_menu_bar(base, None, &[]))
                main class="welcome-page-main welcome-state" {
                    (render_welcome_heading(quotes))
                    (render_recent_card(recent_notes, base))
                    a class="back-link" href=(base.url(return_to)) { "Back" }
                }
                (render_status_bar(recent_notes.len(), theme_name))
                script {
                    "window.__INITIAL_PREVIEW__ = ``;"
                    "window.__BASE_PATH__ = `" (PreEscaped(escape_js(base.prefix()))) "`;"
                    (PreEscaped(include_str!("../static/app.js")))
                }
            }
        }
    }
}

fn render_preview(html_content: &str, default_tab: &str, has_note: bool) -> Markup {
    html! {
        @if has_note {
            section class={"preview-pane" (if default_tab == "preview" { " mobile-active" } else { "" })} data-tab-pane="preview"
                x-bind:class="{ 'mobile-active': tab === 'preview' }" {
                div id="preview" class="markdown-body" x-html="previewHtml" {
                    (PreEscaped(html_content))
                }
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
                         on your own hardware, with notes kept in local Markdown files."
                    }
                    p {
                        "The server binds to all interfaces by default, so you can reach your notes \
                         from a phone or another machine on your network. It is protected by a TOTP \
                         code from your authenticator app. Optional weather sends only the location you configure to wttr.in."
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
                        li { "TOTP-locked, with no accounts or telemetry; weather lookup is optional" }
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
                              License v3.0 or later. No telemetry or accounts; weather is opt-in.",
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
