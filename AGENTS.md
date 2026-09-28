# AGENTS.md

Welcome AI assistant! You are working on **Rasuti Notes** (ラスティ Notes)—a server-side rendered (SSR) copyleft Markdown notes application written in Rust.

Rasuti Notes combines **GNU software freedom** (GPL v3.0-or-later) with the **fluid elegance of Apple Notes** and the **predictable utility of classic desktop GUI applications** (menu bars, split panes, and file organization).

---

## 1. Project Philosophy & Identity

* **Name**: Rasuti Notes (ラスティ Notes—from Japanese *Rasuti* / ラスティ for Rusty).
* **Mission**: Reclaiming high-performance Rust web development under copyleft terms (GPLv3+). Rejecting heavy client-side JavaScript frameworks and proprietary note silos in favor of software freedom, privacy, and near-zero memory footprint.
* **UI Paradigm**: **"Apple Elegance meets Classic Desktop GUI"**
* **Apple Notes Polish**: Fluid sidebars, smooth note item previews, refined typography, generous padding, subtle contrast, and graceful theme transitions.
* **Traditional Desktop Utility**: Top menu bar (`File`, `Edit`, `View`, `Theme`, `Help`), quick action toolbars, split-pane resizers, keyboard shortcuts, and visible document state.



---

## 2. UI/UX Design System & Layout Architecture

The application layout follows a classic 3-column / top-menu layout rendered cleanly via SSR:

```
+-----------------------------------------------------------------------------------+
|  [File] [Edit] [View] [Theme] [Help]                  [Rasuti Notes ラスティ v0.1] |  <-- Classic Menu Bar
+-----------------------------------------------------------------------------------+
|  [+ New Note]  [Delete] |  Search...                  | Theme: [ Dark v ] [Preview] |  <-- Apple-style Toolbar
+-------------------+-----------------------------------+---------------------------+
| Sidebar           | Editor Pane                       | Live Preview Pane         |
| ----------------- | --------------------------------- | ------------------------- |
| 📝 Project Notes   | # Welcome to Rasuti Notes          | Welcome to Rasuti Notes   |
|    Sep 28, 2026   |                                   |                           |
|    GPLv3+ Markdown| Fast, copyleft Markdown editor... | Fast, copyleft Markdown...|
|                   |                                   |                           |
| 📝 Rust Snippets  | - [x] High performance SSR        | • [x] High performance SSR|
|    Sep 25, 2026   | - [ ] Zero heavy JS runtime       | • [ ] Zero heavy JS       |
|    fn main() ...  |                                   |                           |
+-------------------+-----------------------------------+---------------------------+
| Status: 2 Notes | Saved | UTF-8 | Markdown | Theme: Dark                        |  <-- Status Bar
+-----------------------------------------------------------------------------------+

```

### Key UX Principles:

1. **Fluid Typography**: Inter/SF Pro or high-quality system sans-serif for the interface; Monospace for Markdown source; clean serif or clean sans for HTML preview.
2. **Subtle Elevation & Borders**: Soft 1px borders using `--ansi-0` / `--app-bg` contrast shades rather than heavy drop shadows.
3. **Apple-style Note List**: Sidebar lists show note title, relative timestamp, and a single line preview snippet in muted text.
4. **Progressive Enhancement**: Full functionality works with zero JavaScript (via HTML forms and page reloads). With lightweight inline JS or HTMX enabled, typing in the editor triggers real-time live preview swaps without full-page refreshes.

---

## 3. Architecture & Tech Stack

* **Language**: Rust (`edition = "2021"`)
* **Server Framework**: **Axum** or **Actix-web**
* **SSR Templating**: **`maud`** (compiled, type-safe HTML macros) or **`askama`**
* **Reactivity / Live Swap**: **HTMX** (or lightweight SSE / fetch script under 5KB) for seamless live preview typing updates and instant theme swapping.
* **Markdown Engine**: `pulldown-cmark` (with table, tasklist, and strikethrough extensions enabled).
* **License**: **GNU General Public License v3.0 or later** (`GPL-3.0-or-later`). All dependency crates in `Cargo.toml` must be compatible with GPLv3+.

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

### Predefined Themes Vector

```rust
pub static PREDEFINED_THEMES: &[Theme] = &[
    Theme {
        name: "Dark",
        fg: "#d0d0d0",
        bg: "#1c1c1c",
        ansi_normal: ["#1c1c1c", "#c50f1f", "#23a523", "#b58900", "#268bd2", "#d33682", "#2aa198", "#d0d0d0"],
        ansi_bright: ["#666666", "#ff6666", "#66ff66", "#ffff66", "#6666ff", "#ff66ff", "#66ffff", "#ffffff"],
    },
    Theme {
        name: "Light",
        fg: "#1c1c1c",
        bg: "#d0d0d0",
        ansi_normal: ["#1c1c1c", "#c50f1f", "#23a523", "#b58900", "#268bd2", "#d33682", "#2aa198", "#d0d0d0"],
        ansi_bright: ["#666666", "#ff6666", "#66ff66", "#ffff66", "#6666ff", "#ff66ff", "#66ffff", "#ffffff"],
    },
    Theme {
        name: "Solarized Dark",
        fg: "#839496",
        bg: "#002b36",
        ansi_normal: ["#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5"],
        ansi_bright: ["#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3"],
    },
    Theme {
        name: "Solarized Light",
        fg: "#657b83",
        bg: "#fdf6e3",
        ansi_normal: ["#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5"],
        ansi_bright: ["#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3"],
    },
    Theme {
        name: "Dracula",
        fg: "#f8f8f2",
        bg: "#282a36",
        ansi_normal: ["#21222c", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd", "#f8f8f2"],
        ansi_bright: ["#6272a4", "#ff6e6e", "#69ff94", "#ffffa5", "#d6acff", "#ff92df", "#a4ffff", "#ffffff"],
    },
    Theme {
        name: "Gruvbox Dark",
        fg: "#ebdbb2",
        bg: "#282828",
        ansi_normal: ["#282828", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#a89984"],
        ansi_bright: ["#928374", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#ebdbb2"],
    },
    Theme {
        name: "Gruvbox Light",
        fg: "#3c3836",
        bg: "#fbf1c7",
        ansi_normal: ["#fbf1c7", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#7c6f64"],
        ansi_bright: ["#928374", "#9d0006", "#79740e", "#b57614", "#076678", "#8f3f71", "#427b58", "#3c3836"],
    },
    Theme {
        name: "Nord",
        fg: "#d8dee9",
        bg: "#2e3440",
        ansi_normal: ["#3b4252", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#88c0d0", "#e5e9f0"],
        ansi_bright: ["#4c566a", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#8fbcbb", "#eceff4"],
    },
    Theme {
        name: "Monokai",
        fg: "#f8f8f2",
        bg: "#272822",
        ansi_normal: ["#272822", "#f92672", "#a6e22e", "#f4bf75", "#66d9ef", "#ae81ff", "#a1efe4", "#f8f8f2"],
        ansi_bright: ["#75715e", "#f92672", "#a6e22e", "#e6db74", "#66d9ef", "#ae81ff", "#a1efe4", "#f9f8f5"],
    },
    Theme {
        name: "Tokyo Night",
        fg: "#c0caf5",
        bg: "#1a1b26",
        ansi_normal: ["#15161e", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#a9b1d6"],
        ansi_bright: ["#414868", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#c0caf5"],
    },
    Theme {
        name: "One Dark",
        fg: "#abb2bf",
        bg: "#282c34",
        ansi_normal: ["#282c34", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#abb2bf"],
        ansi_bright: ["#5c6370", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#ffffff"],
    },
    Theme {
        name: "Cyberpunk 2077",
        fg: "#f9f227",
        bg: "#0d0d0d",
        ansi_normal: ["#0d0d0d", "#ff0055", "#00ff41", "#f9f227", "#00b8ff", "#ff00cc", "#00ffff", "#f9f227"],
        ansi_bright: ["#333333", "#ff3377", "#33ff66", "#ffff33", "#33ccff", "#ff33ff", "#33ffff", "#ffff66"],
    },
    Theme {
        name: "Cyan on Black",
        fg: "#00ffff",
        bg: "#000000",
        ansi_normal: ["#000000", "#cc0000", "#00cc00", "#cccc00", "#0000cc", "#cc00cc", "#00cccc", "#cccccc"],
        ansi_bright: ["#666666", "#ff0000", "#00ff00", "#ffff00", "#0000ff", "#ff00ff", "#00ffff", "#ffffff"],
    },
    Theme {
        name: "Red on Black",
        fg: "#ff0000",
        bg: "#000000",
        ansi_normal: ["#000000", "#cc0000", "#00cc00", "#cccc00", "#0000cc", "#cc00cc", "#00cccc", "#cccccc"],
        ansi_bright: ["#666666", "#ff0000", "#00ff00", "#ffff00", "#0000ff", "#ff00ff", "#00ffff", "#ffffff"],
    },
    Theme {
        name: "Lime Green on Black",
        fg: "#00ff00",
        bg: "#000000",
        ansi_normal: ["#000000", "#cc0000", "#00cc00", "#cccc00", "#0000cc", "#cc00cc", "#00cccc", "#cccccc"],
        ansi_bright: ["#666666", "#ff0000", "#00ff00", "#ffff00", "#0000ff", "#ff00ff", "#00ffff", "#ffffff"],
    },
    Theme {
        name: "Chartreuse on Black",
        fg: "#7fff00",
        bg: "#000000",
        ansi_normal: ["#000000", "#cc0000", "#00cc00", "#cccc00", "#0000cc", "#cc00cc", "#00cccc", "#cccccc"],
        ansi_bright: ["#666666", "#ff0000", "#00ff00", "#ffff00", "#0000ff", "#ff00ff", "#00ffff", "#ffffff"],
    },
    Theme {
        name: "Amber on Black",
        fg: "#ffbf00",
        bg: "#000000",
        ansi_normal: ["#000000", "#cc0000", "#00cc00", "#cccc00", "#0000cc", "#cc00cc", "#00cccc", "#cccccc"],
        ansi_bright: ["#666666", "#ff0000", "#00ff00", "#ffff00", "#0000ff", "#ff00ff", "#00ffff", "#ffffff"],
    },
    Theme {
        name: "Blueberry on Black",
        fg: "#8ec5fc",
        bg: "#000000",
        ansi_normal: ["#000000", "#0d47a1", "#1565c0", "#1976d2", "#1e88e5", "#42a5f5", "#64b5f6", "#90caf9"],
        ansi_bright: ["#37474f", "#1e88e5", "#42a5f5", "#64b5f6", "#90caf9", "#bbdefb", "#e3f2fd", "#ffffff"],
    },
    Theme {
        name: "Spring Blossom",
        fg: "#ffb7c5",
        bg: "#2d5016",
        ansi_normal: ["#2d5016", "#c2185b", "#7cb342", "#fbc02d", "#00897b", "#d81b60", "#aed581", "#f8bbd0"],
        ansi_bright: ["#558b2f", "#e91e63", "#8bc34a", "#ffeb3b", "#009688", "#ec407a", "#c5e1a5", "#fce4ec"],
    },
    Theme {
        name: "Summer Sunset",
        fg: "#ff9a5a",
        bg: "#1a1a2e",
        ansi_normal: ["#1a1a2e", "#e64a19", "#ff7043", "#ffab40", "#d81b60", "#8e24aa", "#5c6bc0", "#ffb74d"],
        ansi_bright: ["#424242", "#ff5722", "#ff8a65", "#ffc107", "#e91e63", "#9c27b0", "#3f51b5", "#ffe0b2"],
    },
    Theme {
        name: "Autumn Forest",
        fg: "#d4a373",
        bg: "#3e2723",
        ansi_normal: ["#3e2723", "#bf360c", "#e65100", "#8d6e63", "#5d4037", "#33691e", "#9e9d24", "#a1887f"],
        ansi_bright: ["#6d4c41", "#e64a19", "#f57c00", "#a1887f", "#795548", "#558b2f", "#c0ca33", "#d7ccc8"],
    },
    Theme {
        name: "Winter Frost",
        fg: "#e0f7fa",
        bg: "#1a237e",
        ansi_normal: ["#1a237e", "#006064", "#00838f", "#0097a7", "#00acc1", "#26c6da", "#4dd0e1", "#80deea"],
        ansi_bright: ["#283593", "#00897b", "#00acc1", "#26c6da", "#4dd0e1", "#80deea", "#b2ebf2", "#ffffff"],
    },
    Theme {
        name: "Stormy Night",
        fg: "#b0bec5",
        bg: "#263238",
        ansi_normal: ["#263238", "#37474f", "#455a64", "#546e7a", "#607d8b", "#78909c", "#90a4ae", "#b0bec5"],
        ansi_bright: ["#455a64", "#607d8b", "#78909c", "#90a4ae", "#b0bec5", "#cfd8dc", "#eceff1", "#ffffff"],
    },
    Theme {
        name: "Sunny Day",
        fg: "#fff59d",
        bg: "#33691e",
        ansi_normal: ["#33691e", "#f57f17", "#c0ca33", "#00acc1", "#00897b", "#5c6bc0", "#7cb342", "#fff59d"],
        ansi_bright: ["#558b2f", "#fbc02d", "#afb42b", "#26c6da", "#0097a7", "#3f51b5", "#9ccc65", "#fff9c4"],
    },
    Theme {
        name: "Foggy Morning",
        fg: "#cfd8dc",
        bg: "#455a64",
        ansi_normal: ["#455a64", "#546e7a", "#607d8b", "#78909c", "#90a4ae", "#b0bec5", "#cfd8dc", "#eceff1"],
        ansi_bright: ["#607d8b", "#78909c", "#90a4ae", "#b0bec5", "#cfd8dc", "#eceff1", "#f5f5f5", "#ffffff"],
    },
];

```

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
               --ansi-0: {}; --ansi-1: {}; --ansi-2: {}; --ansi-3: {};\n\
               --ansi-4: {}; --ansi-5: {}; --ansi-6: {}; --ansi-7: {};\n\
               --ansi-8: {}; --ansi-9: {}; --ansi-10: {}; --ansi-11: {};\n\
               --ansi-12: {}; --ansi-13: {}; --ansi-14: {}; --ansi-15: {};\n\
             }}",
            self.fg, self.bg,
            self.fg, self.fg,
            self.ansi_normal[0], self.ansi_normal[1], self.ansi_normal[2], self.ansi_normal[3],
            self.ansi_normal[4], self.ansi_normal[5], self.ansi_normal[6], self.ansi_normal[7],
            self.ansi_bright[0], self.ansi_bright[1], self.ansi_bright[2], self.ansi_bright[3],
            self.ansi_bright[4], self.ansi_bright[5], self.ansi_bright[6], self.ansi_bright[7],
        )
    }
}

```

---

## 6. Rules & Directives for AI Developers

1. **Licensing**: Keep all dependencies compatible with **GNU GPLv3+**. Verify crate licenses before introducing new Cargo dependencies.
2. **SSR First**: Ensure basic editing and rendering functionality operates without JavaScript enabled. Use standard HTML forms, query parameters, or cookies for persistence.
3. **Fluid Micro-interactions**: When applying CSS, use smooth `transition: background-color 0.15s ease, color 0.15s ease` so theme switching feels fluid and premium.
4. **Clean Code**: Enforce strict idiomatic Rust (`cargo clippy -- -D warnings`). Avoid `unsafe` blocks.
