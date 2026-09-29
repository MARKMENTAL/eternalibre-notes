# AGENTS.md

Welcome AI assistant! You are working on **EternaLibre Notes** — a server-side rendered (SSR) copyleft Markdown notes application written in Rust.

EternaLibre Notes combines **GNU software freedom** (GPL v3.0-or-later) with the **fluid elegance of Apple Notes** and the **predictable utility of classic desktop GUI applications** (menu bars, split panes, and file organization).

The name splits into its two halves: **Eterna** for permanence and **Libre** for freedom. Notes you keep forever, software that stays free. It borrows Apple's polish and then does the opposite of what Apple does with your data.

---

## 1. Project Philosophy & Identity

* **Name**: EternaLibre Notes — "Free notes, forever."
* **Mission**: Reclaiming high-performance Rust web development under copyleft terms (GPLv3+). Rejecting heavy client-side JavaScript frameworks and proprietary note silos in favor of software freedom, privacy, and near-zero memory footprint.
* **UI Paradigm**: **"Apple Elegance meets Classic Desktop GUI"**
* **Apple Notes Polish**: Fluid sidebars, smooth note item previews, refined typography, generous padding, subtle contrast, and graceful theme transitions.
* **Traditional Desktop Utility**: Top menu bar (`File`, `Edit`, `View`, `Theme`, `Help`), quick action toolbars, split-pane resizers, keyboard shortcuts, and visible document state.

---

## 2. UI/UX Design System & Layout Architecture

The application layout follows a classic 3-column / top-menu layout rendered cleanly via SSR:

```
+-----------------------------------------------------------------------------------+
|  [File] [Edit] [View] [Theme] [Help]        [EternaLibre Notes v0.1]                  |  <-- Classic Menu Bar
+-----------------------------------------------------------------------------------+
|  [+ New Note]  [Delete] |  Search...                  | Theme: [ Dark v ] [Preview] |  <-- Apple-style Toolbar
+-------------------+-----------------------------------+---------------------------+
| Sidebar           | Editor Pane                       | Live Preview Pane         |
| ----------------- | --------------------------------- | ------------------------- |
| Notes             | [Untitled              ]       | Welcome to EternaLibre Notes   |
|  Project Notes    |                                   |                           |
|   Sep 28, 2026    | # Welcome                        | Fast, copyleft Markdown... |
|   GPLv3+ Markdown |                                   |                           |
|   fn main() ...   | Fast, copyleft Markdown editor... |                           |
+-------------------+-----------------------------------+---------------------------+
| Status: 2 Notes | Saved | UTF-8 | Markdown | Theme: Dark                        |  <-- Status Bar
+-----------------------------------------------------------------------------------+

```

### Key UX Principles:

1. **Fluid Typography**: Inter/SF Pro or high-quality system sans-serif for the interface; Monospace for Markdown source; clean sans for HTML preview.
2. **Subtle Elevation & Borders**: Soft 1px borders using `--ui-border` / `--app-bg` contrast shades rather than heavy drop shadows.
3. **Apple-style Note List**: Sidebar lists show note title, relative timestamp, and a single line preview snippet in muted text.
4. **Progressive Enhancement**: Note editing, search, theming, and the login form all work with JavaScript disabled, via HTML forms and page reloads. Three things are JS-dependent by design: the menu bar (dropdowns open from an Alpine-driven `.open` class), the mobile tab bar, and live preview.

---

## 3. Architecture & Tech Stack

* **Language**: Rust (`edition = "2021"`)
* **Server Framework**: **Axum 0.7** (async, Tower ecosystem)
* **Auth**: `totp-rs` 6 (TOTP), `qrcode` 0.14 (inline SVG QR), `rand` 0.10 (session tokens)
* **SSR Templating**: **`maud` 0.26** (compiled, type-safe HTML macros)
* **Reactivity / Live Swap**: **Alpine.js 3.14** (vendored locally in `static/`, ~15 KB)
* **Markdown Engine**: `pulldown-cmark` (with table, tasklist, strikethrough, and footnote extensions)
* **Syntax Highlighting**: `syntect` 5 (for fenced code blocks, themed per active palette)
* **Serialization**: `serde` / `serde_json`
* **Dates**: `chrono`
* **IDs**: `uuid` v4
* **Error Handling**: `anyhow` for application errors, `thiserror` for library-style errors
* **CLI Parsing**: `clap` 4 (derive API)
* **Static Assets**: `tower-http` `ServeDir` for `/static`
* **License**: **GNU General Public License v3.0 or later** (`GPL-3.0-or-later`). All dependency crates in `Cargo.toml` must be compatible with GPLv3+.

### Actual Project Structure

```
eternalibre-notes/
├── Cargo.toml
├── LICENSE                     # GPL-3.0 full text
├── README.md
├── notes/                      # User data: one .md file per note
├── src/
│   ├── main.rs                 # Entry point, CLI args, terminal setup, router assembly
│   ├── lib.rs                  # Library root, so tests/ can drive the HTTP layer
│   ├── auth.rs                 # TOTP secret store, session store, verification
│   ├── setup.rs                # First-run terminal enrolment (QR to stdout, reads stdin)
│   ├── routes.rs               # HTTP routes, auth gate, base-path joining
│   ├── pages.rs                # Maud SSR templates + the `Base` URL helper
│   ├── notes.rs                # Note model + file-system persistence
│   ├── markdown.rs             # Markdown → HTML with code block interception
│   ├── syntax.rs               # syntect engine + per-theme tmTheme generation
│   ├── themes.rs               # 24 predefined themes + CSS variable engine
│   └── storage.rs              # Storage re-export module
├── static/
│   ├── alpine.min.js           # Vendored Alpine.js
│   ├── app.js                  # Alpine component
│   └── style.css               # Full application stylesheet
└── tests/
    └── auth_gating.rs          # End-to-end tests for the TOTP auth gate
```

---

## 4. Theme System & ANSI Palette Data

Theme definitions provide background, foreground, and full 16-color ANSI palettes.

### Rust Data Structures (`src/themes.rs`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Theme {
    pub name: &'static str,
    pub fg: &'static str,
    pub bg: &'static str,
    pub ansi_normal: [&'static str; 8],
    pub ansi_bright: [&'static str; 8],
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ANSIPalette {
    pub name: &'static str,
    pub ansi_normal: [&'static str; 8],
    pub ansi_bright: [&'static str; 8],
}
```

`PREDEFINED_THEMES` contains 24 themes: Dark, Light, Solarized Dark, Solarized Light, Dracula, Gruvbox Dark, Gruvbox Light, Nord, Monokai, Tokyo Night, One Dark, Cyberpunk 2077, Cyan on Black, Red on Black, Lime Green on Black, Chartreuse on Black, Amber on Black, Blueberry on Black, Spring Blossom, Summer Sunset, Autumn Forest, Winter Frost, Stormy Night, and Foggy Morning.

"Sunny Day" was removed: a mid-green background with a pale yellow foreground left almost nothing legible on it, and no amount of syntax tuning rescues a palette that bad.

### Theme Selection

The active theme is stored in a `theme` cookie. The server reads it from the `Cookie` header on every request and injects the matching CSS variable block into the `<head>`. No JavaScript is required to change themes — a plain `<form method="post" action="/theme">` sets a new cookie and redirects.

---

## 5. CSS Var Binding Engine

Server renders the theme colors dynamically into CSS variables powering both the classic GUI chrome (menu bar, status bar) and Apple-style polished UI elements (cards, text fields, focus rings):

```rust
impl Theme {
    pub fn to_css_block(&self) -> String {
        format!(
            ":root {{\n\
               --app-fg: {};\n\
               --app-bg: {};\n\
               --ui-border: {}33;\n\
               --ui-hover: {}1a;\n\
               --btn-primary-bg: {};\n\
               --btn-primary-fg: {};\n\
               --btn-danger-bg: {};\n\
               --btn-danger-fg: {};\n\
               --ansi-0: {}; --ansi-1: {}; --ansi-2: {}; --ansi-3: {};\n\
               --ansi-4: {}; --ansi-5: {}; --ansi-6: {}; --ansi-7: {};\n\
               --ansi-8: {}; --ansi-9: {}; --ansi-10: {}; --ansi-11: {};\n\
               --ansi-12: {}; --ansi-13: {}; --ansi-14: {}; --ansi-15: {};\n\
             }}",
            ...
        )
    }
}
```

### Accessibility Guard: Button Color Derivation

Some themes ship accent colors that are nearly invisible on their own background. Rather than hand-tuning every theme, `Theme::button_colors(accent)` computes a relative luminance and falls back to the theme foreground when an accent is too dark to read:

```rust
fn button_colors(&self, accent: usize) -> (&'static str, &'static str) {
    let bright = self.ansi_bright[accent];
    if relative_luminance(bright) < 0.15 {
        (self.fg, self.bg)
    } else {
        (bright, self.bg)
    }
}
```

Themes that previously used `#0000ff` on a pure black background were also corrected at the source: that slot is now `#a9a9f9` in the affected palettes (Cyan on Black, Red on Black, Lime Green on Black, Chartreuse on Black, and Amber on Black).

---

## 6. Syntax Highlighting

`src/syntax.rs` generates a `.tmTheme` plist **at runtime** from each EternaLibre theme, then loads it into `syntect`. This means code highlighting always matches the active palette instead of shipping one fixed color scheme.

```rust
pub struct SyntaxState {
    pub syntax_set: SyntaxSet,
    pub themes: HashMap<String, SyntectTheme>,
}
```

The state is held in a `OnceLock` and initialized eagerly at startup so the first request is not slow. `src/markdown.rs` walks the `pulldown-cmark` event stream, intercepts `Event::Start(Tag::CodeBlock(..))`, collects the text, highlights it, and re-emits it as `Event::Html`. Unknown language tags fall back to plain text.

Scope → ANSI mapping, by ANSI slot:

| Scope | Slot |
|-------|------|
| `comment` | black (see below) + italic |
| `string` | green |
| `constant.numeric` / `constant` | yellow |
| `keyword` / `storage` | magenta |
| `entity.name.function` / `support.function` | blue |
| `entity.name.type` | cyan |
| `variable` / `invalid` | red |

### Colors are chosen by measured contrast, not by index

An earlier revision hardcoded the **bright** variant of every slot. That is correct for a dark background and disastrous for a light one: the Light theme rendered `#66ff66` strings against a `#d0d0d0` background at **1.17:1**, which is not "low contrast", it is invisible. Nothing about the palette data flagged it — both colors were valid hex.

`syntax_color` therefore picks between `ansi_normal[slot]` and `ansi_bright[slot]` by **WCAG contrast ratio against the theme background**, and `comment_color` does the same over the two blacks plus the foreground. This is the same reasoning as the `button_colors` luminance guard in §5: prefer the measured property over a hand-maintained table.

Two things to know if you touch this:

* **`syntax.rs` has its own `relative_luminance`, and it is not the one in `themes.rs`.** The `themes.rs` version sums gamma-encoded channels and is only good enough for the coarse `< 0.15` button guard. Contrast ratios need the linearized sRGB curve, or the numbers are wrong in precisely the range that matters (dark colors on light backgrounds). They are deliberately different functions with nearly identical names.
* **Candidates are sorted by contrast, not kept in a fixed order.** Which ANSI black is the dimmer one flips between light and dark backgrounds, so a hardcoded preference order silently picks the loud option on half the palettes.

### Comments optimize for the *opposite* end of the tradeoff

`syntax_color` takes the **most** readable candidate. `comment_color` takes the **least** prominent one that still clears `COMMENT_MIN_CONTRAST` (2.5:1). Maximizing comment contrast is maximally legible and practically useless — on the Light theme it paints every comment the same color as the surrounding prose, so a commented block becomes a wall of text with no hierarchy left to scan.

Where no candidate clears the floor, the palette is at fault, not the code. **Solarized Dark ships `#002b36` as its bright black, which is exactly its background** — comments there were not faint, they were invisible at 1.00:1. The fallback to the foreground is what rescues it (4.86:1).

Comments also carry `fontStyle: italic` in the tmTheme, which syntect renders as `font-style:italic` on the span. Color alone is not a reliable signal, since a comment that falls back to the foreground is by definition the same color as the prose.

The three failure modes are each pinned by a test, and each test was verified to fail when the corresponding bug is reintroduced: `comments_are_readable_in_every_theme`, `no_theme_uses_its_background_for_comments`, and `comments_stay_distinct_from_the_foreground_when_they_can`. `syntax_color_always_picks_the_more_readable_variant` additionally pins the selection rule itself across all 24 themes and all 8 slots.

### The Light theme has a data ceiling, not a logic bug

`Light` is a literal inversion of `Dark` — the same 16 colors with `fg` and `bg` swapped, on a mid-gray `#d0d0d0` background. After this change its worst scope bottoms out at **2.05:1** (type/cyan, `#2aa198`), because no slot in that palette offers a darker or more saturated cyan. The selection logic is already returning the best available option; the palette is what limits it.

So **do not "fix" a low-contrast Light theme by changing the selection logic** — it is provably optimal per slot, and `syntax_color_always_picks_the_more_readable_variant` will fail if it stops being so. The only real remedies are palette changes: lightening the background to near-white lifts type/cyan from 2.05:1 to 3.16:1 and string/green from 2.10:1 to 3.24:1, or replacing the normal ANSI colors outright. Both change how the whole application looks, not just code, so they are a design decision rather than a bug fix.

---

## 7. Responsive / Mobile Design

Desktop keeps the 3-column grid. At `max-width: 600px` (which covers iPhone 11 at 390px CSS width) the layout collapses to a **tabbed single-pane interface** with a **Notes / Write / Preview** tab bar.

Implementation notes:
* The whole page is a flex column using `height: 100dvh`, so the mobile browser chrome that hides on scroll does not break the layout. This replaced an earlier brittle `calc(100vh - 32px - 44px - 24px)` height calculation.
* The default tab (`write` when a note is open, `notes` otherwise) is rendered **server-side** onto the correct pane as `mobile-active`, so the layout still works with JavaScript disabled. Alpine only takes over switching afterward.
* Touch targets are at least 44px tall (Apple HIG minimum). Inputs and textareas use a 16px font size on mobile to prevent iOS Safari zoom-on-focus.
* The toolbar wraps rather than overflowing, and `min-width: 0` is set on flex children so they shrink instead of stretching off-screen.

### CSS Stacking Context Rule

`overflow` on an **ancestor** creates a new stacking context, which traps absolutely positioned descendants. An earlier revision set `overflow-x: auto` on `.menu-list` to allow horizontal menu scrolling, which silently pushed the menu dropdowns *underneath* the toolbar. The fix is `flex-wrap: wrap` with `overflow: visible`.

> **Do not reintroduce `overflow`, `transform`, `filter`, or `opacity` on `.menu-bar`, `.menu-list`, or `.menu-item`.** `overflow` on the dropdown *itself* is safe (it only clips its own contents), but on an ancestor it traps the dropdown.

The 24-entry Theme menu can exceed the viewport height, so `.menu-dropdown` itself carries `max-height` plus `overflow-y: auto`. This is the safe location for that property.

### Every screen needs a viewport meta, or the mobile CSS is dead

**`/about` shipped without `<meta name="viewport">` while every other screen had it.** Without it, mobile browsers fall back to a ~980px layout viewport and scale the page down. Two things follow, and both are invisible until you know to look:

1. **`@media (max-width: 600px)` never matches.** The query evaluates against the 980px layout viewport, not the real screen width.
2. **The desktop layout is what renders**, scaled down — which reads as "assumes a desktop view."

The trap is that the CSS is *correct and present*. You can add a complete mobile stylesheet, see it inlined in the served page, confirm every rule and value is right, and the page still shows the desktop layout because the media query never fires. Two rounds of CSS work were spent on `/about` before the missing meta tag was spotted.

**When a screen's responsive CSS appears to do nothing, check the viewport meta before touching the CSS.**

`render_app`, `render_auth_page`, and `render_about` each render their own `<head>`, so a tag added to one is not inherited by the others. `every_screen_declares_a_viewport` in `tests/auth_gating.rs` checks `/` and `/about` per-screen for this reason; add new screens to that list.

### The About page is a separate document, not the app shell

`/about` renders its own `<html>` via `render_about` and inlines `style.css` itself. It reuses `render_menu_bar` and `render_status_bar` but has no `.toolbar` and no `.tab-bar`, so its chrome deliberately does not match the notes view.

**Its version and theme counts are derived, never typed.** The version comes from `env!("CARGO_PKG_VERSION")`, and the same macro drives the `.app-title` in the menu bar — which previously carried a hand-written `"v0.1"` that had already drifted from the crate's `0.1.0`. The theme line is `PREDEFINED_THEMES.len()` split by `Theme::is_dark()`, so adding or removing a palette needs no edit here. Both are pinned by `about_page_reports_the_manifest_version` and `about_page_theme_counts_come_from_the_palette`.

`Theme::is_dark` classifies on the local gamma-encoded `relative_luminance` rather than the linearised one in `syntax.rs`. That is safe *only* because the shipped palettes are strongly bimodal: the lightest dark background scores 0.095 and the darkest light one 0.63, so any threshold in that gap agrees. If a future palette lands near 0.5, switch this to the WCAG function rather than nudging the threshold — the two agree on all 24 themes today, so the current answer is not a close call.

**Every outbound link carries `rel="noopener noreferrer"`.** These are the only links that leave the machine, and the app advertises that it never talks to a third party. `about_page_external_links_are_hardened` walks every `<a href="https://` in the rendered page, so a link added later without the attributes fails rather than shipping quietly. It caught the Help menu's GPL link, which had `target="_blank"` and no `rel`.

**It is not missing a `.tab-bar` because that is a bug.** The tab bar exists to switch between the notes/editor/preview panes; the about page has no panes. Bolting it on would be chrome for its own sake. The menu bar *is* kept, because the app keeps it on mobile too.

The one thing it was genuinely missing was typography. It was the only screen with no rules of its own, so it rendered at UA defaults — a 2em heading with 0.67em margins and a 1.9 line-height feature list — which read as a desktop page on a phone. It now has a base `h1`/`p` treatment plus a `≤600px` override set (smaller heading and tagline, 1.6 line-height list, 20px padding, and a full-width 44px `.back-link` matching the HIG minimum the rest of the app enforces).

`render_about` must keep `include_str!("../static/style.css")` and the `class="back-link"` on the anchor. `about_page_inlines_the_stylesheet` and `about_page_back_link_is_styled` in `tests/auth_gating.rs` guard both — the latter because the mobile button style hangs entirely off that one class, so dropping it silently reverts to a 13px text link.

---

## 8. Menu Bar Behavior

Menus are opened by **click**, not CSS `:hover`, driven by a single Alpine state variable. Keeping one name in one slot makes mutual exclusion automatic — opening Theme while Help is open simply reassigns the value, so Help hides itself with no extra bookkeeping.

```javascript
openMenu: null,
toggleMenu(name) {
  this.openMenu = this.openMenu === name ? null : name;
},
```

Dropdowns are revealed by a class on the trigger rather than `x-show`, because Alpine's `x-show` works by *removing* an inline `display` when true, at which point the stylesheet's `display: none` would take over again and the menu would stay hidden. Driving it from `.menu-item.open .menu-dropdown` also means the menus stay closed if JavaScript never loads, instead of all five appearing at once.

**Consequence: the menu bar requires JavaScript.** Since `.open` is applied by Alpine, no dropdown opens without it. This is a deliberate trade-off (see section 2, directive 4) — note CRUD, search, and theming are unaffected. If you change this, keep `.desktop-only` and the `.open` rule consistent; see section 8.

Outside-click detection is a manual `document` listener in an `init()` hook with a matching `destroy()`. `Escape` closes any open menu.

### Maud + Alpine Modifier Caveat

**Maud treats `.` in an attribute name as CSS class shorthand.** `x-on:click.outside="..."` parses as the attribute `x-on:click` plus a class named `outside`, producing a `duplicate attribute class` compile error. The same applies to `x-on:input.debounce.300ms` and `x-on:click.stop`. Use plain Alpine directives in Maud templates and bind modifiers manually in `static/app.js`.

---

## 9. Routes

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/login` | TOTP unlock form (public) |
| `POST` | `/login` | Verify code, issue session (public) |
| `POST` | `/logout` | Drop the session server-side and clear the cookie |
| `GET` | `/` | App shell with note list (accepts `?q=` for search) |
| `GET` | `/notes/:id` | App shell with a note open in the editor |
| `POST` | `/notes` | Create a new note, redirect to it |
| `POST` | `/notes/:id` | Update or delete based on the `_method` form field |
| `POST` | `/preview` | Returns rendered HTML for live preview |
| `POST` | `/theme` | Sets the `theme` cookie, redirects to `/` |
| `GET` | `/about` | Static about page |
| `GET` | `/export` | Downloads all notes as a single Markdown file |
| `*` | `/static/*` | Vendored Alpine.js, app.js, and style.css |

Notes are stored as `<uuid>.md` files in `notes/` with YAML frontmatter carrying `title`, `created`, and `updated` timestamps. When no frontmatter is present, the title is derived from the first `# ` heading. The directory is overridable via the `--notes-dir` flag or the `ETERNALIBRE_NOTES_DIR` env var, which is how the integration tests stay out of real user data.

---

## 9b. TOTP Authentication

Every route except `/login` and `/logout` sits behind a `route_layer` middleware. This is a hard gate: an unauthenticated caller gets `303` to `/login` and never reaches a handler that could observe note content.

### Enrolment happens in the terminal, not on a page

**There is no `/setup` route.** `setup::run_interactive_setup` runs in `main()` *before* the listener binds, prints the QR as `Dense1x2` blocks (plus the Base32 secret and `otpauth://` URI as manual-entry fallbacks), and blocks on stdin for the code. Only then does the socket open.

This is the security property, not a convenience: an HTTP-reachable enrolment endpoint means anyone who can reach the port before the operator finishes could claim the device. Because setup completes before `axum::serve` is called, that window does not exist. There is a test asserting `GET /setup` returns 404.

If the secret file is deleted while the server runs, `/login` returns 503 with a "restart and complete setup in the terminal" message rather than redirecting to a dead route.

### Design decisions

**The secret is only committed after a successful verification.** `begin_setup` generates a secret and stashes it as `pending_secret` in memory. `verify_setup` checks the entered code against that pending secret and only then writes `.totp_secret` to disk (mode `0600`). A half-finished setup therefore cannot lock anyone out, and a wrong code leaves no file behind.

**Regenerating setup is safe.** Every call to `begin_setup` replaces `pending_secret` and invalidates any previously displayed QR code. The code on screen is always the one that will work.

**Sessions are in-memory only.** A restart drops every session, which matches the "authenticate on launch" requirement. Tokens are 32 hex-encoded bytes from the OS-seeded generator, held in a `HashMap<String, Instant>` behind an `RwLock`. Expired entries are pruned on every validation.

**`/preview` returns 401, not a redirect.** It is fetched by `fetch()` from the live-preview JS. Redirecting would hand that `fetch` the login HTML, and the `x-html` swap would silently paste a login page into the preview pane. A 401 fails loudly instead.

**Logout forgets the token server-side**, not just the cookie. Clearing only the cookie would leave a captured copy of the token valid for the rest of its hour.

**A corrupt `.totp_secret` fails closed.** `verify_code` parses the secret before looking at the code, so a truncated or corrupted file makes every login impossible rather than accepting anything.

**Recovery is deleting the file.** `rm .totp_secret` and restart reopens the terminal enrolment flow. There is no recovery-code fallback by design — the file *is* the recovery mechanism.

### Session cookie

```
eternalibre_session=<64 hex chars>; Path=/; HttpOnly; SameSite=Strict; Max-Age=3600
```

`Secure` is deliberately omitted so the app also works over plain HTTP on a trusted LAN, which is the intended deployment (it binds `0.0.0.0` by default). `Path` becomes the base path when one is configured; see 9c.

### skew

`with_skew(1)` accepts the neighbouring 30s step, so roughly ±30s of clock drift still validates. Two steps out is rejected — there are tests pinning both edges of that window.

---

## 9c. Base Path (reverse-proxy subdirectory)

`--base-path /forgejo` makes the app answer behind a reverse proxy that mounts it at a subdirectory. Empty (the default) means the site root, which is what most installs use and is unchanged behaviour.

**The app is mounted at the proxy's root.** It receives app-absolute paths (`/about`, not `/forgejo/about`) because `ProxyPass` strips the prefix. The base path is used *only when generating URLs*: hrefs, form actions, the static asset `src`, and every `Location` header.

```apache
ProxyPass        /forgejo/  http://127.0.0.1:3000/  timeout=10
ProxyPassReverse /forgejo/  http://127.0.0.1:3000/
```

Consequences that are easy to get wrong:

- **Cookie `Path` is scoped to the prefix** (`Path=/forgejo`), so the session token is not sent to sibling apps on the same host. At the root it is `/`, the default-path.
- **Static assets stay app-absolute** (`nest_service("/static")` in `routes::app`). This is the one place the base path is *not* applied, and getting it wrong is subtle: the asset 404s, `x-data="app()"` never initialises, and every Alpine directive — including the File menu dropdowns — silently stops working. There is a test asserting `GET /static/app.js` returns 200 while a base path is set. `app.js` is inlined via `include_str!` and has no URL to rewrite; only `alpine.min.js` and `style.css` are externally loaded.
- **Curl will 404 on prefixed paths.** Hitting `http://host:3000/forgejo/login` directly bypasses Apache, so nothing strips the prefix. Test through the proxy, or curl the app-absolute path.
- **Never hardcode a path in JS.** `static/app.js` matches the new-note form with `form[action$="/notes"]` rather than an exact attribute, so `Ctrl+N` keeps working under any prefix. The one `fetch()` it makes is different, because the rule above does not cover it:

  ```javascript
  const base = window.__BASE_PATH__ || '';
  fetch(base + '/preview', { ... });
  ```

  `app.js` is inlined into the page, so it cannot call the server-side `base.url()` helper that every form action and asset `src` uses. A literal `fetch('/preview')` resolves against the *origin root*, which behind a proxy mounted at `/forgejo/` never reaches the app. The failure is nasty because the proxy returns its own 404 body, which `x-html` then swaps straight into the preview pane — the user sees "404 Not Found" in the middle of their document while typing, with nothing in the server logs to explain it. `render_app` therefore injects `window.__BASE_PATH__` next to `__INITIAL_PREVIEW__`, escaped by the same `escape_js` helper and emitted as a backtick literal like its neighbour.

  Pinned by `the_base_path_is_handed_to_client_side_js` and `the_preview_fetch_uses_the_injected_base_path` in `tests/auth_gating.rs`. The second matters most: `app.js` is inlined verbatim, so a regression there is invisible to every other test in the suite.

- **A redirect target is not a forwarded path.** A `303` to `{base}/` is correct for the browser and 404s if you curl it directly, because nothing strips the prefix. This also bites reverse-proxy test harnesses: a proxy built on `urllib`/`requests` will silently follow the redirect, re-resolve it against the upstream root, and return a 404 that looks like an app bug. Disable redirect following when testing through a proxy.

---

## 10. CLI

```
Usage: eternalibre-notes [OPTIONS]

Options:
      --host <HOST>  Host address to bind to [default: 0.0.0.0]
  -p, --port <PORT>  Port to listen on [default: 3000]
      --notes-dir <NOTES_DIR>  Directory holding note files [default: notes]
      --base-path <BASE_PATH>  Path prefix when served behind a reverse proxy subdirectory [default: ]
  -h, --help         Print help
```

Binding to `0.0.0.0` by default makes the app reachable from other devices on the local network, which is how you would use it from a phone. Because the listener is exposed to the network, TOTP is not optional — it is the only thing between an unauthenticated peer and your notes.

---

## 11. Rules & Directives for AI Developers

1. **Licensing**: Keep all dependencies compatible with **GNU GPLv3+**. Verify crate licenses before introducing new Cargo dependencies.
2. **SSR First**: Ensure basic editing and rendering functionality operates without JavaScript enabled. Use standard HTML forms, query parameters, or cookies for persistence.
3. **Fluid Micro-interactions**: When applying CSS, use smooth `transition: background-color 0.15s ease, color 0.15s ease` so theme switching feels fluid and premium.
4. **Clean Code**: Enforce strict idiomatic Rust (`cargo clippy -- -D warnings`). Avoid `unsafe` blocks.
5. **Respect the stacking context rule**: See section 7 before touching overflow or transform on menu-related selectors.
6. **Remember the Maud attribute caveat**: See section 8 before adding any Alpine modifier to a template.
7. **Test before claiming done**: Run `cargo fmt -- --check`, `cargo clippy -- -D warnings`, `cargo test`, and `cargo build --release`. If a browser is unavailable, say so rather than implying the UI was visually verified.
