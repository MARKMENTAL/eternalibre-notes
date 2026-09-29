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

impl Default for SyntaxState {
    fn default() -> Self {
        Self::new()
    }
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
        .expect("Syntect theme should exist for every EternaLibre theme");

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

/// WCAG 2.x relative luminance.
///
/// Deliberately *not* the same as `themes::relative_luminance`, which sums
/// gamma-encoded channels and is only good enough for the coarse
/// `button_colors` guard. Contrast ratios need the real sRGB curve, or the
/// numbers are wrong in exactly the range we care about (dark colors on
/// light backgrounds).
fn relative_luminance(hex: &str) -> f64 {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return 0.0;
    }
    let channel = |i: usize| -> f64 {
        let raw = u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0) as f64 / 255.0;
        if raw <= 0.040_45 {
            raw / 12.92
        } else {
            ((raw + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
}

/// WCAG contrast ratio between two opaque colors. Ranges 1.0 to 21.0.
fn contrast_ratio(a: &str, b: &str) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Picks the more readable of an ANSI slot's normal and bright variants.
///
/// The bright variants are tuned for dark backgrounds. Using them
/// unconditionally meant light themes rendered `#66ff66` strings against a
/// `#d0d0d0` background — 1.17:1, effectively invisible. Choosing by
/// measured contrast fixes every light theme at once instead of hand-tuning
/// each palette.
fn syntax_color(theme: &Theme, slot: usize) -> &'static str {
    let normal = theme.ansi_normal[slot];
    let bright = theme.ansi_bright[slot];
    if contrast_ratio(normal, theme.bg) >= contrast_ratio(bright, theme.bg) {
        normal
    } else {
        bright
    }
}

/// The contrast a comment must reach before we consider it readable. Below
/// this a comment is not "muted", it is gone.
const COMMENT_MIN_CONTRAST: f64 = 2.5;

/// Picks a comment color that is readable *and* still recedes.
///
/// The rule is deliberately the inverse of [`syntax_color`]: among the
/// candidates, take the **least** prominent one that still clears
/// [`COMMENT_MIN_CONTRAST`]. Taking the *most* prominent one is maximally
/// legible and practically useless — on the Light theme it paints every
/// comment the same color as the surrounding prose, so a commented block
/// becomes a wall of text with no hierarchy left to scan.
///
/// Candidates are sorted by measured contrast rather than kept in a fixed
/// order, because which ANSI black is the dimmer one flips between light
/// and dark backgrounds.
fn comment_color(theme: &Theme) -> String {
    let mut candidates = [theme.ansi_normal[0], theme.ansi_bright[0], theme.fg];
    candidates.sort_by(|a, b| {
        contrast_ratio(a, theme.bg)
            .partial_cmp(&contrast_ratio(b, theme.bg))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Ascending sort, so the first match is the dimmest readable option and
    // the last element is the most legible thing the palette has.
    candidates
        .iter()
        .find(|c| contrast_ratio(c, theme.bg) >= COMMENT_MIN_CONTRAST)
        .copied()
        .or_else(|| candidates.last().copied())
        .unwrap_or(theme.fg)
        .to_string()
}

fn generate_tmtheme(theme: &Theme) -> String {
    let scopes: [(&str, String); 11] = [
        ("comment", comment_color(theme)),
        ("string", syntax_color(theme, 2).to_string()),
        ("constant.numeric", syntax_color(theme, 3).to_string()),
        ("constant", syntax_color(theme, 3).to_string()),
        ("keyword", syntax_color(theme, 5).to_string()),
        ("storage", syntax_color(theme, 5).to_string()),
        ("entity.name.function", syntax_color(theme, 4).to_string()),
        ("support.function", syntax_color(theme, 4).to_string()),
        ("entity.name.type", syntax_color(theme, 6).to_string()),
        ("variable", syntax_color(theme, 1).to_string()),
        ("invalid", syntax_color(theme, 1).to_string()),
    ];

    let mut scope_entries = String::new();
    for (scope, color) in &scopes {
        // Comments carry a font style as well as a colour. Colour alone is a
        // poor signal — a comment that falls back to the foreground is the
        // same colour as the prose, and italic is the distinction every
        // editor uses.
        let font_style = if *scope == "comment" {
            "\n             <key>fontStyle</key>\n             <string>italic</string>"
        } else {
            ""
        };
        scope_entries.push_str(&format!(
            "    <dict>\n\
             <key>name</key>\n\
             <string>{scope}</string>\n\
             <key>scope</key>\n\
             <string>{scope}</string>\n\
             <key>settings</key>\n\
             <dict>\n\
             <key>foreground</key>\n\
             <string>{color}</string>{font_style}\n\
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
         <string>EternaLibre {}</string>\n\
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::get_theme;

    /// The regression that motivated all of this: Solarized Dark ships
    /// `#002b36` as its bright black, which is *exactly* its background, so
    /// comments rendered completely invisible.
    #[test]
    fn comments_are_readable_in_every_theme() {
        for theme in PREDEFINED_THEMES {
            let color = comment_color(theme);
            let ratio = contrast_ratio(&color, theme.bg);
            assert!(
                ratio >= COMMENT_MIN_CONTRAST,
                "{}: comment {color} on {} is only {ratio:.2}:1",
                theme.name,
                theme.bg
            );
        }
    }

    /// The specific Solarized Dark failure deserves its own assertion, since
    /// it is the worst possible outcome: a comment you cannot see at all.
    #[test]
    fn no_theme_uses_its_background_for_comments() {
        for theme in PREDEFINED_THEMES {
            assert_ne!(
                comment_color(theme).to_lowercase(),
                theme.bg.to_lowercase(),
                "{}: comment color is the background color",
                theme.name
            );
        }
    }

    /// A comment that is merely readable is not enough if it is as loud as
    /// the code around it. Guard the property the max-contrast rule broke.
    #[test]
    fn comments_stay_distinct_from_the_foreground_when_they_can() {
        // On Light, `#1c1c1c` reads fine but is the prose color; the muted
        // `#666666` should win because it clears the floor.
        let light = get_theme("Light");
        assert_eq!(comment_color(light), light.ansi_bright[0]);
        assert!(contrast_ratio(&comment_color(light), light.bg) < 8.0);
    }

    /// The contract `syntax_color` actually promises: it returns the more
    /// readable of the two candidates. Checking this over every theme and
    /// every slot pins the rule itself, so it cannot silently regress to
    /// "always bright" without a failure naming the offending theme.
    #[test]
    fn syntax_color_always_picks_the_more_readable_variant() {
        for theme in PREDEFINED_THEMES {
            for slot in 0..8 {
                let picked = syntax_color(theme, slot);
                let normal = contrast_ratio(theme.ansi_normal[slot], theme.bg);
                let bright = contrast_ratio(theme.ansi_bright[slot], theme.bg);
                let best = normal.max(bright);
                let got = contrast_ratio(picked, theme.bg);
                assert!(
                    (got - best).abs() < 1e-9,
                    "{} slot {slot}: picked {picked} at {got:.2}:1 but the \
                     better option is {best:.2}:1",
                    theme.name
                );
            }
        }
    }

    /// Light themes used to render their bright, high-luminance variants,
    /// which put `#66ff66` strings on a light gray background at 1.17:1.
    ///
    /// The 2.0:1 floor is deliberately low. The Light palette is a literal
    /// inversion of Dark (same 16 colors, fg and bg swapped) on a mid-gray
    /// `#d0d0d0` background, and its darkest available cyan tops out at
    /// 2.05:1. No selection logic can do better without changing the
    /// palette. This test guards against a *regression* to the old
    /// bright-variant behavior (1.27:1 for that same slot), not against the
    /// palette being mediocre.
    #[test]
    fn light_theme_syntax_colors_are_readable() {
        let light = get_theme("Light");
        // Slots 1-6 are the ones actually bound to a scope; slot 0 is
        // handled by `comment_color` and slot 7 is unused.
        for slot in 1..=6 {
            let color = syntax_color(light, slot);
            let ratio = contrast_ratio(color, light.bg);
            assert!(
                ratio >= 2.0,
                "Light slot {slot} {color} is only {ratio:.2}:1 on {}",
                light.bg
            );
        }
    }

    /// Comment scope carries an italic font style; the rest do not.
    #[test]
    fn comment_scope_is_italic_and_others_are_not() {
        let plist = generate_tmtheme(get_theme("Dark"));
        let comment = scope_entry(&plist, "comment");
        assert!(
            comment.contains("<key>fontStyle</key>") && comment.contains("italic"),
            "comment scope should be italic"
        );
        assert!(
            !scope_entry(&plist, "string").contains("fontStyle"),
            "only comments should be italic"
        );
    }

    /// The italic has to survive the whole pipeline, not just parse. If
    /// syntect ever stops emitting `font-style`, this catches it.
    #[test]
    fn comments_render_italic_in_the_final_html() {
        let state = syntax_state();
        let html = highlight_code("// hi\n", Some("rust"), get_theme("Dark"), state);
        assert!(
            html.contains("font-style:italic"),
            "expected italic comments in rendered output, got: {html}"
        );
    }

    /// Hand-editing the plist template is exactly the kind of change that
    /// silently emits invalid XML that syntect then rejects at load time.
    #[test]
    fn every_theme_generates_a_loadable_tmtheme() {
        for theme in PREDEFINED_THEMES {
            let plist = generate_tmtheme(theme);
            ThemeSet::load_from_reader(&mut std::io::Cursor::new(plist))
                .unwrap_or_else(|e| panic!("{}: {e}", theme.name));
        }
    }

    /// Pull one scope's `<dict>` block out of the generated plist.
    fn scope_entry(plist: &str, scope: &str) -> String {
        let marker = format!("<string>{scope}</string>");
        let start = plist
            .find(&marker)
            .unwrap_or_else(|| panic!("no scope entry for {scope}"));
        let rest = &plist[start..];
        let end = rest.find("</dict>").expect("unterminated scope entry");
        rest[..end].to_string()
    }
}
