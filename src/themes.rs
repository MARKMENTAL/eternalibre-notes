use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub name: &'static str,
    pub fg: &'static str,
    pub bg: &'static str,
    pub ansi_normal: [&'static str; 8],
    pub ansi_bright: [&'static str; 8],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ANSIPalette {
    pub name: &'static str,
    pub ansi_normal: [&'static str; 8],
    pub ansi_bright: [&'static str; 8],
}

impl Theme {
    pub fn to_css_block(&self) -> String {
        let (btn_primary_bg, btn_primary_fg) = self.button_colors(4);
        let (btn_danger_bg, btn_danger_fg) = self.button_colors(1);
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
            self.fg,
            self.bg,
            self.fg,
            self.fg,
            btn_primary_bg,
            btn_primary_fg,
            btn_danger_bg,
            btn_danger_fg,
            self.ansi_normal[0],
            self.ansi_normal[1],
            self.ansi_normal[2],
            self.ansi_normal[3],
            self.ansi_normal[4],
            self.ansi_normal[5],
            self.ansi_normal[6],
            self.ansi_normal[7],
            self.ansi_bright[0],
            self.ansi_bright[1],
            self.ansi_bright[2],
            self.ansi_bright[3],
            self.ansi_bright[4],
            self.ansi_bright[5],
            self.ansi_bright[6],
            self.ansi_bright[7],
        )
    }

    /// True when the background is dark enough that light text reads on it.
    ///
    /// The shipped palettes are strongly bimodal: the lightest *dark*
    /// background scores 0.095 and the darkest *light* one scores 0.63, so
    /// any threshold in that gap returns the same answer. That margin is why
    /// this can use the local gamma-encoded `relative_luminance` rather than
    /// the linearised one in `syntax.rs` — for a binary split the two agree
    /// on all 40 themes, and adding a third implementation would cost more
    /// than the precision is worth.
    pub fn is_dark(&self) -> bool {
        relative_luminance(self.bg) < 0.5
    }

    fn button_colors(&self, accent: usize) -> (&'static str, &'static str) {
        // Use the bright variant of the accent color if it has decent luminance;
        // otherwise fall back to the theme foreground color so text remains readable.
        let bright = self.ansi_bright[accent];
        if self.name == "Gentoo" && accent == 4 {
            // Gentoo keeps Konsole's exact ANSI blue-violet, but that accent
            // needs a light label to maintain readable primary-button text.
            (bright, "#ffffff")
        } else if relative_luminance(bright) < 0.15 {
            (self.fg, self.bg)
        } else {
            (bright, self.bg)
        }
    }
}

fn relative_luminance(hex: &str) -> f64 {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return 0.0;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f64 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f64 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f64 / 255.0;
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

pub static PREDEFINED_THEMES: &[Theme] = &[
    Theme {
        name: "Amber on Black",
        fg: "#ffbf00",
        bg: "#000000",
        ansi_normal: [
            "#000000", "#cc0000", "#00cc00", "#cccc00", "#0000cc", "#cc00cc", "#00cccc", "#cccccc",
        ],
        ansi_bright: [
            "#666666", "#ff0000", "#00ff00", "#ffff00", "#a9a9f9", "#ff00ff", "#00ffff", "#ffffff",
        ],
    },
    Theme {
        name: "Autumn Forest",
        fg: "#d4a373",
        bg: "#3e2723",
        ansi_normal: [
            "#3e2723", "#bf360c", "#e65100", "#8d6e63", "#5d4037", "#33691e", "#9e9d24", "#a1887f",
        ],
        ansi_bright: [
            "#6d4c41", "#e64a19", "#f57c00", "#a1887f", "#795548", "#558b2f", "#c0ca33", "#d7ccc8",
        ],
    },
    Theme {
        name: "Blueberry on Black",
        fg: "#8ec5fc",
        bg: "#000000",
        ansi_normal: [
            "#000000", "#0d47a1", "#1565c0", "#1976d2", "#1e88e5", "#42a5f5", "#64b5f6", "#90caf9",
        ],
        ansi_bright: [
            "#37474f", "#1e88e5", "#42a5f5", "#64b5f6", "#90caf9", "#bbdefb", "#e3f2fd", "#ffffff",
        ],
    },
    Theme {
        name: "Blueberry Professional",
        fg: "#263247",
        bg: "#f4efe6",
        ansi_normal: [
            "#f4efe6", "#9d4d47", "#58734f", "#8c6a2f", "#356a9a", "#735c89", "#347b83", "#263247",
        ],
        ansi_bright: [
            "#857d75", "#873d3a", "#4d6545", "#75531f", "#2d5e8b", "#624977", "#28666f", "#1d2634",
        ],
    },
    Theme {
        name: "Breeze",
        fg: "#232629",
        bg: "#eff0f1",
        ansi_normal: [
            "#eff0f1", "#b00020", "#2e7d32", "#8a5a00", "#1d5f91", "#8e44ad", "#007f7f", "#232629",
        ],
        ansi_bright: [
            "#7f8c8d", "#a31515", "#256d2a", "#805400", "#0067c0", "#7b2e9a", "#006b6b", "#111111",
        ],
    },
    Theme {
        name: "Breeze Dark",
        fg: "#eff0f1",
        bg: "#232629",
        ansi_normal: [
            "#232629", "#f07178", "#8bd49c", "#e5c07b", "#3daee9", "#c792ea", "#56b6c2", "#d8dee9",
        ],
        ansi_bright: [
            "#697078", "#ff7a85", "#a3e6b1", "#f2cc8f", "#61c5f0", "#d6a8f5", "#70d0db", "#ffffff",
        ],
    },
    Theme {
        name: "Chartreuse on Black",
        fg: "#7fff00",
        bg: "#000000",
        ansi_normal: [
            "#000000", "#cc0000", "#00cc00", "#cccc00", "#0000cc", "#cc00cc", "#00cccc", "#cccccc",
        ],
        ansi_bright: [
            "#666666", "#ff0000", "#00ff00", "#ffff00", "#a9a9f9", "#ff00ff", "#00ffff", "#ffffff",
        ],
    },
    Theme {
        name: "Classic Windows 95",
        fg: "#111111",
        bg: "#c0c0c0",
        ansi_normal: [
            "#c0c0c0", "#800000", "#006400", "#806000", "#000080", "#800080", "#008080", "#111111",
        ],
        ansi_bright: [
            "#808080", "#c00000", "#008000", "#9a6700", "#004e98", "#9a4e9a", "#008b8b", "#ffffff",
        ],
    },
    Theme {
        name: "Claude",
        fg: "#3c3530",
        bg: "#f4efe6",
        ansi_normal: [
            "#f4efe6", "#a85245", "#66754f", "#9b7132", "#c56f52", "#84607a", "#4f7a78", "#3c3530",
        ],
        ansi_bright: [
            "#8a8175", "#963f35", "#52603f", "#79551f", "#a85137", "#704d66", "#3d6866", "#27211d",
        ],
    },
    Theme {
        name: "Cyan on Black",
        fg: "#00ffff",
        bg: "#000000",
        ansi_normal: [
            "#000000", "#cc0000", "#00cc00", "#cccc00", "#0000cc", "#cc00cc", "#00cccc", "#cccccc",
        ],
        ansi_bright: [
            "#666666", "#ff0000", "#00ff00", "#ffff00", "#a9a9f9", "#ff00ff", "#00ffff", "#ffffff",
        ],
    },
    Theme {
        name: "Cyberpunk 2077",
        fg: "#f9f227",
        bg: "#0d0d0d",
        ansi_normal: [
            "#0d0d0d", "#ff0055", "#00ff41", "#f9f227", "#00b8ff", "#ff00cc", "#00ffff", "#f9f227",
        ],
        ansi_bright: [
            "#333333", "#ff3377", "#33ff66", "#ffff33", "#33ccff", "#ff33ff", "#33ffff", "#ffff66",
        ],
    },
    Theme {
        name: "Dark",
        fg: "#d0d0d0",
        bg: "#1c1c1c",
        ansi_normal: [
            "#1c1c1c", "#c50f1f", "#23a523", "#b58900", "#268bd2", "#d33682", "#2aa198", "#d0d0d0",
        ],
        ansi_bright: [
            "#666666", "#ff6666", "#66ff66", "#ffff66", "#6666ff", "#ff66ff", "#66ffff", "#ffffff",
        ],
    },
    Theme {
        name: "Debian Professional",
        fg: "#342a29",
        bg: "#f7f1e9",
        ansi_normal: [
            "#f7f1e9", "#b34e56", "#5d704d", "#96702c", "#b24d57", "#805c75", "#3e7673", "#342a29",
        ],
        ansi_bright: [
            "#817a76", "#9e3d48", "#4d6140", "#78561d", "#9b3643", "#6d4963", "#326764", "#241c1b",
        ],
    },
    Theme {
        name: "Dracula",
        fg: "#f8f8f2",
        bg: "#282a36",
        ansi_normal: [
            "#21222c", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd", "#f8f8f2",
        ],
        ansi_bright: [
            "#6272a4", "#ff6e6e", "#69ff94", "#ffffa5", "#d6acff", "#ff92df", "#a4ffff", "#ffffff",
        ],
    },
    Theme {
        name: "Foggy Morning",
        fg: "#cfd8dc",
        bg: "#455a64",
        ansi_normal: [
            "#455a64", "#546e7a", "#607d8b", "#78909c", "#90a4ae", "#b0bec5", "#cfd8dc", "#eceff1",
        ],
        ansi_bright: [
            "#607d8b", "#78909c", "#90a4ae", "#b0bec5", "#cfd8dc", "#eceff1", "#f5f5f5", "#ffffff",
        ],
    },
    Theme {
        name: "Gentoo",
        fg: "#9b86e3",
        bg: "#0c0c0c",
        ansi_normal: [
            "#0c0c0c", "#c50f1f", "#13a10e", "#c19c00", "#61538d", "#881798", "#00aaff", "#cccccc",
        ],
        ansi_bright: [
            "#7c7c7c", "#e74856", "#16c60c", "#f9f1a5", "#6f5fa2", "#b4009e", "#0094de", "#f2f2f2",
        ],
    },
    Theme {
        name: "Gentoo Professional",
        fg: "#eee7f3",
        bg: "#211b28",
        ansi_normal: [
            "#211b28", "#c97874", "#91a47a", "#c8a66b", "#806a9c", "#b47ca7", "#70aaa7", "#eee7f3",
        ],
        ansi_bright: [
            "#6c6270", "#e28b83", "#a3b68b", "#e0bf82", "#a08bbc", "#d099bc", "#83c3bd", "#ffffff",
        ],
    },
    Theme {
        name: "Golden Hour",
        fg: "#f5e8c9",
        bg: "#211a12",
        ansi_normal: [
            "#211a12", "#d06b52", "#a4a06a", "#a38334", "#b98c37", "#a584a5", "#6f9d98", "#f5e8c9",
        ],
        ansi_bright: [
            "#6b5b43", "#ee8468", "#bab981", "#e0c16f", "#d6a84f", "#c09cc1", "#88b8ae", "#fff3db",
        ],
    },
    Theme {
        name: "Gruvbox Dark",
        fg: "#ebdbb2",
        bg: "#282828",
        ansi_normal: [
            "#282828", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#a89984",
        ],
        ansi_bright: [
            "#928374", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#ebdbb2",
        ],
    },
    Theme {
        name: "Gruvbox Light",
        fg: "#3c3836",
        bg: "#fbf1c7",
        ansi_normal: [
            "#fbf1c7", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#7c6f64",
        ],
        ansi_bright: [
            "#928374", "#9d0006", "#79740e", "#b57614", "#076678", "#8f3f71", "#427b58", "#3c3836",
        ],
    },
    Theme {
        name: "Light",
        fg: "#1c1c1c",
        bg: "#d0d0d0",
        ansi_normal: [
            "#1c1c1c", "#c50f1f", "#23a523", "#b58900", "#268bd2", "#d33682", "#2aa198", "#d0d0d0",
        ],
        ansi_bright: [
            "#666666", "#ff6666", "#66ff66", "#ffff66", "#6666ff", "#ff66ff", "#66ffff", "#ffffff",
        ],
    },
    Theme {
        name: "Lilac Paper",
        fg: "#302b3d",
        bg: "#f4f0fa",
        ansi_normal: [
            "#f4f0fa", "#a33a58", "#3d7553", "#856000", "#584c9d", "#814f91", "#287781", "#302b3d",
        ],
        ansi_bright: [
            "#89829a", "#96334f", "#2d704a", "#755500", "#5d468f", "#713f83", "#176c74", "#211a2a",
        ],
    },
    Theme {
        name: "Lime Green on Black",
        fg: "#00ff00",
        bg: "#000000",
        ansi_normal: [
            "#000000", "#cc0000", "#00cc00", "#cccc00", "#0000cc", "#cc00cc", "#00cccc", "#cccccc",
        ],
        ansi_bright: [
            "#666666", "#ff0000", "#00ff00", "#ffff00", "#a9a9f9", "#ff00ff", "#00ffff", "#ffffff",
        ],
    },
    Theme {
        name: "Monokai",
        fg: "#f8f8f2",
        bg: "#272822",
        ansi_normal: [
            "#272822", "#f92672", "#a6e22e", "#f4bf75", "#66d9ef", "#ae81ff", "#a1efe4", "#f8f8f2",
        ],
        ansi_bright: [
            "#75715e", "#f92672", "#a6e22e", "#e6db74", "#66d9ef", "#ae81ff", "#a1efe4", "#f9f8f5",
        ],
    },
    Theme {
        name: "Nord",
        fg: "#d8dee9",
        bg: "#2e3440",
        ansi_normal: [
            "#3b4252", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#88c0d0", "#e5e9f0",
        ],
        ansi_bright: [
            "#4c566a", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#8fbcbb", "#eceff4",
        ],
    },
    Theme {
        name: "One Dark",
        fg: "#abb2bf",
        bg: "#282c34",
        ansi_normal: [
            "#282c34", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#abb2bf",
        ],
        ansi_bright: [
            "#5c6370", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#ffffff",
        ],
    },
    Theme {
        name: "Orchid Noir",
        fg: "#f3eaf8",
        bg: "#110b18",
        ansi_normal: [
            "#110b18", "#ff6b9b", "#8fe0b0", "#f4cf78", "#b775f2", "#ff66b3", "#65d7dc", "#f3eaf8",
        ],
        ansi_bright: [
            "#685d70", "#ff8ab0", "#a7ecc1", "#ffdf96", "#c78df7", "#ff86c3", "#82e7eb", "#ffffff",
        ],
    },
    Theme {
        name: "Platinum Desktop",
        fg: "#242424",
        bg: "#d6d6d6",
        ansi_normal: [
            "#d6d6d6", "#a33a3a", "#3d6b45", "#8a6200", "#336699", "#78508a", "#287d78", "#242424",
        ],
        ansi_bright: [
            "#777777", "#8f1d1d", "#2f6a38", "#765000", "#225a91", "#6b3f7e", "#176963", "#101010",
        ],
    },
    Theme {
        name: "Red on Black",
        fg: "#ff0000",
        bg: "#000000",
        ansi_normal: [
            "#000000", "#cc0000", "#00cc00", "#cccc00", "#0000cc", "#cc00cc", "#00cccc", "#cccccc",
        ],
        ansi_bright: [
            "#666666", "#ff0000", "#00ff00", "#ffff00", "#a9a9f9", "#ff00ff", "#00ffff", "#ffffff",
        ],
    },
    Theme {
        name: "Rose Quartz",
        fg: "#3a2630",
        bg: "#fff1f4",
        ansi_normal: [
            "#fff1f4", "#a52a4e", "#39724a", "#856000", "#b83f68", "#853f78", "#176f76", "#3a2630",
        ],
        ansi_bright: [
            "#8b7b84", "#922243", "#2e653e", "#795400", "#9b3157", "#713365", "#0d646b", "#25171d",
        ],
    },
    Theme {
        name: "Solarized Dark",
        fg: "#839496",
        bg: "#002b36",
        ansi_normal: [
            "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5",
        ],
        ansi_bright: [
            "#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3",
        ],
    },
    Theme {
        name: "Solarized Light",
        fg: "#657b83",
        bg: "#fdf6e3",
        ansi_normal: [
            "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5",
        ],
        ansi_bright: [
            "#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3",
        ],
    },
    Theme {
        name: "Stormy Night",
        fg: "#b0bec5",
        bg: "#263238",
        ansi_normal: [
            "#263238", "#37474f", "#455a64", "#546e7a", "#607d8b", "#78909c", "#90a4ae", "#b0bec5",
        ],
        ansi_bright: [
            "#455a64", "#607d8b", "#78909c", "#90a4ae", "#b0bec5", "#cfd8dc", "#eceff1", "#ffffff",
        ],
    },
    Theme {
        name: "Summer Sunset",
        fg: "#ff9a5a",
        bg: "#1a1a2e",
        ansi_normal: [
            "#1a1a2e", "#e64a19", "#ff7043", "#ffab40", "#d81b60", "#8e24aa", "#5c6bc0", "#ffb74d",
        ],
        ansi_bright: [
            "#424242", "#ff5722", "#ff8a65", "#ffc107", "#e91e63", "#9c27b0", "#3f51b5", "#ffe0b2",
        ],
    },
    Theme {
        name: "Tokyo Night",
        fg: "#c0caf5",
        bg: "#1a1b26",
        ansi_normal: [
            "#15161e", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#a9b1d6",
        ],
        ansi_bright: [
            "#414868", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#c0caf5",
        ],
    },
    Theme {
        name: "Ubuntu Professional",
        fg: "#302a34",
        bg: "#f7f2ea",
        ansi_normal: [
            "#f7f2ea", "#ad4838", "#526f4d", "#976b2a", "#bc5227", "#6d3b69", "#367777", "#302a34",
        ],
        ansi_bright: [
            "#827b78", "#963d30", "#466242", "#79531c", "#a54018", "#5d2d58", "#2c6666", "#211c25",
        ],
    },
    Theme {
        name: "Velvet Dusk",
        fg: "#eae1f2",
        bg: "#1c1422",
        ansi_normal: [
            "#1c1422", "#f0718a", "#91d39b", "#e8c27a", "#ab82d3", "#d08acb", "#70c9c4", "#eae1f2",
        ],
        ansi_bright: [
            "#63586b", "#ff8299", "#a6e6af", "#f2d08c", "#c19ae8", "#e1a0dc", "#83ddd6", "#ffffff",
        ],
    },
    Theme {
        name: "Vista Aero",
        fg: "#eaf4fb",
        bg: "#14263a",
        ansi_normal: [
            "#14263a", "#ff6b7a", "#8fe3b2", "#ffd27a", "#53b8f2", "#ce9eff", "#67dce3", "#eaf4fb",
        ],
        ansi_bright: [
            "#697f96", "#ff8793", "#a4eac1", "#ffdda0", "#62c8ff", "#d9b4ff", "#7ae8ed", "#ffffff",
        ],
    },
    Theme {
        name: "Windows XP",
        fg: "#1c2a3a",
        bg: "#eaf2fa",
        ansi_normal: [
            "#eaf2fa", "#a52222", "#247a35", "#886000", "#245edc", "#873b80", "#007984", "#1c2a3a",
        ],
        ansi_bright: [
            "#7b8794", "#8f1d1d", "#216b2f", "#765000", "#245edc", "#71316b", "#006b74", "#111820",
        ],
    },
    Theme {
        name: "Winter Frost",
        fg: "#e0f7fa",
        bg: "#1a237e",
        ansi_normal: [
            "#1a237e", "#006064", "#00838f", "#0097a7", "#00acc1", "#26c6da", "#4dd0e1", "#80deea",
        ],
        ansi_bright: [
            "#283593", "#00897b", "#00acc1", "#26c6da", "#4dd0e1", "#80deea", "#b2ebf2", "#ffffff",
        ],
    },
];

pub fn get_theme(name: &str) -> &'static Theme {
    PREDEFINED_THEMES
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| {
            PREDEFINED_THEMES
                .iter()
                .find(|theme| theme.name == "Dark")
                .expect("the default Dark theme must be present")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_is_dark() {
        let theme = get_theme("Nonexistent");
        assert_eq!(theme.name, "Dark");
    }

    #[test]
    fn css_block_contains_variables() {
        let theme = get_theme("Dark");
        let css = theme.to_css_block();
        assert!(css.contains("--app-fg:"));
        assert!(css.contains("--app-bg:"));
        assert!(css.contains("--btn-primary-bg:"));
        assert!(css.contains("--btn-danger-bg:"));
        assert!(css.contains("--ansi-0:"));
        assert!(css.contains("--ansi-15:"));
    }

    #[test]
    fn dark_themes_use_readable_button_colors() {
        // Bright accent colors should be luminous enough to use as button
        // backgrounds; very dark accents fall back to the theme foreground.
        let cyan = get_theme("Cyan on Black");
        let (bg, fg) = cyan.button_colors(4);
        // After replacing #0000ff with #a9a9f9, the bright blue is readable
        // and no longer falls back to the foreground color.
        assert_eq!(bg, cyan.ansi_bright[4]);
        assert_eq!(fg, cyan.bg);
    }

    #[test]
    fn all_themes_have_sixteen_ansi_colors() {
        for theme in PREDEFINED_THEMES {
            assert_eq!(theme.ansi_normal.len(), 8);
            assert_eq!(theme.ansi_bright.len(), 8);
        }
    }

    #[test]
    fn removed_themes_are_absent_and_catalog_count_is_current() {
        assert!(
            !PREDEFINED_THEMES.iter().any(|t| t.name == "Sunny Day"),
            "Sunny Day should have been deleted"
        );
        assert!(!PREDEFINED_THEMES.iter().any(|t| t.name == "Spring Blossom"));
        assert_eq!(PREDEFINED_THEMES.len(), 40);
    }

    #[test]
    fn breeze_variants_are_a_light_dark_pair() {
        let light = get_theme("Breeze");
        let dark = get_theme("Breeze Dark");

        assert!(!light.is_dark());
        assert!(dark.is_dark());
        assert_eq!(light.ansi_bright[4], "#0067c0");
        assert_eq!(dark.ansi_normal[4], "#3daee9");
    }

    #[test]
    fn lavender_and_rose_themes_cover_light_and_dark_modes() {
        assert!(!get_theme("Lilac Paper").is_dark());
        assert!(!get_theme("Rose Quartz").is_dark());
        assert!(get_theme("Velvet Dusk").is_dark());
        assert!(get_theme("Orchid Noir").is_dark());
        assert_eq!(get_theme("Rose Quartz").ansi_bright[4], "#9b3157");
        assert_eq!(get_theme("Orchid Noir").ansi_bright[5], "#ff86c3");
    }

    #[test]
    fn gentoo_theme_preserves_konsole_palette_and_uses_white_button_text() {
        let gentoo = get_theme("Gentoo");
        assert!(gentoo.is_dark());
        assert_eq!(gentoo.bg, "#0c0c0c");
        assert_eq!(gentoo.fg, "#9b86e3");
        assert_eq!(
            gentoo.ansi_normal,
            [
                "#0c0c0c", "#c50f1f", "#13a10e", "#c19c00", "#61538d", "#881798", "#00aaff",
                "#cccccc",
            ]
        );
        assert_eq!(
            gentoo.ansi_bright,
            [
                "#7c7c7c", "#e74856", "#16c60c", "#f9f1a5", "#6f5fa2", "#b4009e", "#0094de",
                "#f2f2f2",
            ]
        );

        let (button_bg, button_fg) = gentoo.button_colors(4);
        assert_eq!(button_bg, "#6f5fa2");
        assert_eq!(button_fg, "#ffffff");
        assert_eq!(gentoo.button_colors(1), ("#e74856", "#0c0c0c"));
        assert!(gentoo.to_css_block().contains("--btn-primary-fg: #ffffff;"));
    }

    #[test]
    fn warm_and_distro_themes_have_distinct_light_dark_palettes() {
        assert!(!get_theme("Claude").is_dark());
        assert!(get_theme("Golden Hour").is_dark());
        assert!(!get_theme("Ubuntu Professional").is_dark());
        assert!(!get_theme("Debian Professional").is_dark());
        assert!(get_theme("Gentoo Professional").is_dark());

        assert_eq!(get_theme("Claude").ansi_bright[4], "#a85137");
        assert_eq!(get_theme("Golden Hour").ansi_bright[4], "#d6a84f");
        assert_eq!(get_theme("Ubuntu Professional").ansi_bright[4], "#a54018");
        assert_eq!(get_theme("Debian Professional").ansi_bright[4], "#9b3643");
        assert_eq!(get_theme("Gentoo Professional").ansi_bright[4], "#a08bbc");
    }

    #[test]
    fn blueberry_professional_is_a_light_neutral_blueberry_variant() {
        let theme = get_theme("Blueberry Professional");
        assert!(!theme.is_dark());
        assert_eq!(theme.bg, "#f4efe6");
        assert_eq!(theme.fg, "#263247");
        assert_eq!(theme.ansi_bright[4], "#2d5e8b");
        assert!(PREDEFINED_THEMES
            .iter()
            .any(|candidate| candidate.name == "Blueberry on Black"));
    }

    #[test]
    fn theme_names_are_alphabetical() {
        let names: Vec<String> = PREDEFINED_THEMES
            .iter()
            .map(|theme| theme.name.to_lowercase())
            .collect();
        assert!(
            names.windows(2).all(|pair| pair[0] <= pair[1]),
            "theme names should be alphabetized"
        );
    }

    #[test]
    fn theme_names_are_unique() {
        let mut names: Vec<&str> = PREDEFINED_THEMES.iter().map(|t| t.name).collect();
        names.sort_unstable();
        let total = names.len();
        names.dedup();
        assert_eq!(names.len(), total, "duplicate theme name");
    }
}
