use crate::themes::{Theme, PREDEFINED_THEMES};
use std::collections::HashMap;
use std::sync::OnceLock;
use syntect::highlighting::{Theme as SyntectTheme, ThemeSet};
use syntect::html::highlighted_html_for_string;
use syntect::parsing::SyntaxSet;

pub struct SyntaxState {
    pub syntax_set: SyntaxSet,
    pub themes: HashMap<String, SyntectTheme>,
}

impl SyntaxState {
    pub fn new() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let mut themes = HashMap::new();

        for theme in PREDEFINED_THEMES {
            let tmtheme = generate_tmtheme(theme);
            let syntect_theme = ThemeSet::load_from_reader(&mut std::io::Cursor::new(tmtheme))
                .unwrap_or_else(|e| {
                    eprintln!("Failed to load generated theme for {}: {}", theme.name, e);
                    ThemeSet::load_defaults().themes["InspiredGitHub"].clone()
                });
            themes.insert(theme.name.to_string(), syntect_theme);
        }

        Self { syntax_set, themes }
    }
}

pub fn highlight_code(
    code: &str,
    lang: Option<&str>,
    theme: &Theme,
    state: &SyntaxState,
) -> String {
    let syntect_theme = state
        .themes
        .get(theme.name)
        .expect("Syntect theme should exist for every Rasuti theme");

    let syntax = match lang {
        Some(lang) => state.syntax_set.find_syntax_by_token(lang),
        None => None,
    }
    .unwrap_or_else(|| state.syntax_set.find_syntax_plain_text());

    highlighted_html_for_string(code, &state.syntax_set, syntax, syntect_theme)
        .unwrap_or_else(|_| format!("<pre><code>{}</code></pre>", html_escape(code)))
}

static SYNTAX_STATE: OnceLock<SyntaxState> = OnceLock::new();

pub fn syntax_state() -> &'static SyntaxState {
    SYNTAX_STATE.get_or_init(SyntaxState::new)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn generate_tmtheme(theme: &crate::themes::Theme) -> String {
    let scopes = [
        ("comment", theme.ansi_bright[0]),
        ("string", theme.ansi_bright[2]),
        ("constant.numeric", theme.ansi_bright[3]),
        ("constant", theme.ansi_bright[3]),
        ("keyword", theme.ansi_bright[5]),
        ("storage", theme.ansi_bright[5]),
        ("entity.name.function", theme.ansi_bright[4]),
        ("support.function", theme.ansi_bright[4]),
        ("entity.name.type", theme.ansi_bright[6]),
        ("variable", theme.ansi_bright[1]),
        ("invalid", theme.ansi_bright[1]),
    ];

    let mut scope_entries = String::new();
    for (scope, color) in scopes {
        scope_entries.push_str(&format!(
            "    <dict>\n\
             <key>name</key>\n\
             <string>{scope}</string>\n\
             <key>scope</key>\n\
             <string>{scope}</string>\n\
             <key>settings</key>\n\
             <dict>\n\
             <key>foreground</key>\n\
             <string>{color}</string>\n\
             </dict>\n\
             </dict>\n"
        ));
    }

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         <key>name</key>\n\
         <string>Rasuti {}</string>\n\
         <key>settings</key>\n\
         <array>\n\
         <dict>\n\
         <key>settings</key>\n\
         <dict>\n\
         <key>background</key>\n\
         <string>{}</string>\n\
         <key>foreground</key>\n\
         <string>{}</string>\n\
         </dict>\n\
         </dict>\n\
         {}\
         </array>\n\
         </dict>\n\
         </plist>\n",
        theme.name, theme.bg, theme.fg, scope_entries
    )
}
