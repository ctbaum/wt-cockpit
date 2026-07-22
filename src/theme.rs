//! Herdr-compatible UI palette loading.
//!
//! Herdr does not currently export its resolved palette to popup processes, so
//! the deck reads the same config file and mirrors Herdr 0.7's built-in color
//! tables and custom override precedence.

use ratatui::style::Color;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Palette {
    pub accent: Color,
    pub panel_bg: Color,
    pub surface0: Color,
    pub surface1: Color,
    pub surface_dim: Color,
    pub overlay0: Color,
    pub overlay1: Color,
    pub text: Color,
    pub subtext0: Color,
    pub green: Color,
    pub yellow: Color,
    pub red: Color,
    pub blue: Color,
}

impl Palette {
    pub fn load() -> Self {
        Self::load_from(&config_path()).unwrap_or_else(Self::catppuccin)
    }

    fn load_from(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let config: toml::Value = toml::from_str(&content).ok()?;
        let theme = config.get("theme").and_then(toml::Value::as_table);
        let manual_name = theme
            .and_then(|table| table.get("name"))
            .and_then(toml::Value::as_str)
            .unwrap_or("catppuccin");
        let auto_switch = theme
            .and_then(|table| table.get("auto_switch"))
            .and_then(toml::Value::as_bool)
            .unwrap_or(false);
        // Popup processes do not receive Herdr's live host-appearance state.
        // Herdr also resolves auto-switch to dark until appearance is known.
        let name = if auto_switch {
            theme
                .and_then(|table| table.get("dark_name"))
                .and_then(toml::Value::as_str)
                .unwrap_or_else(|| sibling_theme_names(manual_name).0)
        } else {
            manual_name
        };
        let fallback = if auto_switch {
            "catppuccin"
        } else {
            manual_name
        };
        let mut palette = Self::from_name(name)
            .or_else(|| Self::from_name(fallback))
            .unwrap_or_else(Self::catppuccin);

        let custom = theme
            .and_then(|table| table.get("custom"))
            .and_then(toml::Value::as_table);
        if let Some(custom) = custom {
            palette.apply_overrides(custom);
        }
        if custom.and_then(|table| table.get("accent")).is_none()
            && let Some(accent) = config
                .get("ui")
                .and_then(toml::Value::as_table)
                .and_then(|table| table.get("accent"))
                .and_then(toml::Value::as_str)
            && accent != "cyan"
        {
            palette.accent = parse_color(accent);
        }
        Some(palette)
    }

    pub fn contrast_fg(&self) -> Color {
        if self.panel_bg == Color::Reset {
            self.surface_dim
        } else {
            self.panel_bg
        }
    }

    pub fn terminal() -> Self {
        Self {
            accent: Color::Blue,
            panel_bg: Color::Reset,
            surface0: Color::Reset,
            surface1: Color::DarkGray,
            surface_dim: Color::DarkGray,
            overlay0: Color::Gray,
            overlay1: Color::White,
            text: Color::Reset,
            subtext0: Color::Gray,
            green: Color::Green,
            yellow: Color::Yellow,
            red: Color::LightRed,
            blue: Color::Blue,
        }
    }

    fn rgb(colors: [[u8; 3]; 13]) -> Self {
        let color = |index: usize| {
            let [red, green, blue] = colors[index];
            Color::Rgb(red, green, blue)
        };
        Self {
            accent: color(0),
            panel_bg: color(1),
            surface0: color(2),
            surface1: color(3),
            surface_dim: color(4),
            overlay0: color(5),
            overlay1: color(6),
            text: color(7),
            subtext0: color(8),
            green: color(9),
            yellow: color(10),
            red: color(11),
            blue: color(12),
        }
    }

    fn catppuccin() -> Self {
        Self::rgb([
            [137, 180, 250],
            [24, 24, 37],
            [49, 50, 68],
            [69, 71, 90],
            [30, 30, 46],
            [108, 112, 134],
            [127, 132, 156],
            [205, 214, 244],
            [166, 173, 200],
            [166, 227, 161],
            [249, 226, 175],
            [243, 139, 168],
            [137, 180, 250],
        ])
    }

    fn from_name(name: &str) -> Option<Self> {
        let normalized = normalize_theme_name(name);
        let colors = match normalized.as_str() {
            "catppuccin" | "catppuccin-mocha" => return Some(Self::catppuccin()),
            "terminal" => return Some(Self::terminal()),
            "catppuccin-latte" | "latte" | "light" => [
                [30, 102, 245],
                [239, 241, 245],
                [204, 208, 218],
                [188, 192, 204],
                [230, 233, 239],
                [156, 160, 176],
                [140, 143, 161],
                [76, 79, 105],
                [108, 111, 133],
                [64, 160, 43],
                [223, 142, 29],
                [210, 15, 57],
                [30, 102, 245],
            ],
            "tokyo-night" | "tokyonight" => [
                [122, 162, 247],
                [26, 27, 38],
                [36, 40, 59],
                [65, 72, 104],
                [26, 27, 38],
                [86, 95, 137],
                [105, 113, 150],
                [192, 202, 245],
                [169, 177, 214],
                [158, 206, 106],
                [224, 175, 104],
                [247, 118, 142],
                [122, 162, 247],
            ],
            "tokyo-night-day" | "tokyo-day" | "tokyonight-day" => [
                [46, 125, 233],
                [225, 226, 231],
                [196, 200, 218],
                [168, 174, 203],
                [210, 211, 218],
                [137, 144, 179],
                [104, 112, 154],
                [55, 96, 191],
                [97, 114, 176],
                [88, 117, 57],
                [140, 108, 62],
                [245, 42, 101],
                [46, 125, 233],
            ],
            "dracula" => [
                [189, 147, 249],
                [40, 42, 54],
                [68, 71, 90],
                [98, 114, 164],
                [40, 42, 54],
                [98, 114, 164],
                [130, 140, 180],
                [248, 248, 242],
                [210, 210, 220],
                [80, 250, 123],
                [241, 250, 140],
                [255, 85, 85],
                [139, 233, 253],
            ],
            "nord" => [
                [136, 192, 208],
                [46, 52, 64],
                [59, 66, 82],
                [67, 76, 94],
                [46, 52, 64],
                [76, 86, 106],
                [100, 110, 130],
                [236, 239, 244],
                [216, 222, 233],
                [163, 190, 140],
                [235, 203, 139],
                [191, 97, 106],
                [129, 161, 193],
            ],
            "gruvbox" | "gruvbox-dark" => [
                [215, 153, 33],
                [40, 40, 40],
                [60, 56, 54],
                [80, 73, 69],
                [40, 40, 40],
                [146, 131, 116],
                [168, 153, 132],
                [235, 219, 178],
                [213, 196, 161],
                [184, 187, 38],
                [250, 189, 47],
                [251, 73, 52],
                [131, 165, 152],
            ],
            "gruvbox-light" => [
                [7, 102, 120],
                [251, 241, 199],
                [235, 219, 178],
                [213, 196, 161],
                [242, 229, 188],
                [146, 131, 116],
                [124, 111, 100],
                [60, 56, 54],
                [80, 73, 69],
                [121, 116, 14],
                [181, 118, 20],
                [157, 0, 6],
                [7, 102, 120],
            ],
            "one-dark" | "onedark" => [
                [97, 175, 239],
                [40, 44, 52],
                [44, 49, 58],
                [62, 68, 81],
                [40, 44, 52],
                [92, 99, 112],
                [115, 122, 135],
                [171, 178, 191],
                [150, 156, 168],
                [152, 195, 121],
                [229, 192, 123],
                [224, 108, 117],
                [97, 175, 239],
            ],
            "one-light" | "onelight" => [
                [64, 120, 242],
                [250, 250, 250],
                [240, 240, 241],
                [229, 229, 230],
                [245, 245, 246],
                [160, 161, 167],
                [104, 107, 119],
                [56, 58, 66],
                [104, 107, 119],
                [80, 161, 79],
                [193, 132, 1],
                [228, 86, 73],
                [64, 120, 242],
            ],
            "solarized" | "solarized-dark" => [
                [38, 139, 210],
                [0, 43, 54],
                [7, 54, 66],
                [88, 110, 117],
                [0, 43, 54],
                [88, 110, 117],
                [101, 123, 131],
                [147, 161, 161],
                [131, 148, 150],
                [133, 153, 0],
                [181, 137, 0],
                [220, 50, 47],
                [38, 139, 210],
            ],
            "solarized-light" => [
                [38, 139, 210],
                [253, 246, 227],
                [238, 232, 213],
                [147, 161, 161],
                [238, 232, 213],
                [147, 161, 161],
                [88, 110, 117],
                [101, 123, 131],
                [131, 148, 150],
                [133, 153, 0],
                [181, 137, 0],
                [220, 50, 47],
                [38, 139, 210],
            ],
            "kanagawa" => [
                [126, 156, 216],
                [31, 31, 40],
                [42, 42, 55],
                [54, 54, 70],
                [31, 31, 40],
                [114, 113, 105],
                [135, 134, 125],
                [220, 215, 186],
                [200, 195, 170],
                [118, 148, 106],
                [192, 163, 110],
                [195, 64, 67],
                [126, 156, 216],
            ],
            "kanagawa-lotus" | "lotus" => [
                [77, 105, 155],
                [242, 236, 188],
                [220, 213, 172],
                [201, 203, 209],
                [213, 206, 163],
                [160, 156, 172],
                [138, 137, 128],
                [84, 84, 100],
                [67, 67, 108],
                [111, 137, 78],
                [119, 113, 63],
                [200, 64, 83],
                [77, 105, 155],
            ],
            "rose-pine" | "rosepine" => [
                [196, 167, 231],
                [25, 23, 36],
                [31, 29, 46],
                [38, 35, 58],
                [25, 23, 36],
                [110, 106, 134],
                [144, 140, 170],
                [224, 222, 244],
                [200, 197, 220],
                [49, 116, 143],
                [246, 193, 119],
                [235, 111, 146],
                [49, 116, 143],
            ],
            "rose-pine-dawn" | "rosepine-dawn" | "dawn" => [
                [144, 122, 169],
                [250, 244, 237],
                [242, 233, 225],
                [255, 250, 243],
                [242, 233, 225],
                [152, 147, 165],
                [121, 117, 147],
                [70, 66, 97],
                [121, 117, 147],
                [40, 105, 131],
                [234, 157, 52],
                [180, 99, 122],
                [40, 105, 131],
            ],
            "vesper" => [
                [255, 199, 153],
                [26, 26, 26],
                [35, 35, 35],
                [40, 40, 40],
                [16, 16, 16],
                [92, 92, 92],
                [126, 126, 126],
                [255, 255, 255],
                [160, 160, 160],
                [153, 255, 228],
                [255, 199, 153],
                [255, 128, 128],
                [176, 176, 176],
            ],
            _ => return None,
        };
        Some(Self::rgb(colors))
    }

    fn apply_overrides(&mut self, custom: &toml::map::Map<String, toml::Value>) {
        for (key, value) in custom {
            let Some(value) = value.as_str() else {
                continue;
            };
            let color = parse_color(value);
            match key.as_str() {
                "accent" => self.accent = color,
                "panel_bg" => self.panel_bg = color,
                "surface0" => self.surface0 = color,
                "surface1" => self.surface1 = color,
                "surface_dim" => self.surface_dim = color,
                "overlay0" => self.overlay0 = color,
                "overlay1" => self.overlay1 = color,
                "text" => self.text = color,
                "subtext0" => self.subtext0 = color,
                "green" => self.green = color,
                "yellow" => self.yellow = color,
                "red" => self.red = color,
                "blue" => self.blue = color,
                _ => {}
            }
        }
    }
}

fn config_path() -> PathBuf {
    if let Ok(path) = std::env::var("HERDR_CONFIG_PATH") {
        return PathBuf::from(path);
    }
    if let Ok(directory) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(directory).join("herdr/config.toml");
    }
    PathBuf::from(crate::ext::home()).join(".config/herdr/config.toml")
}

fn normalize_theme_name(name: &str) -> String {
    name.to_lowercase().replace([' ', '_'], "-")
}

fn sibling_theme_names(name: &str) -> (&'static str, &'static str) {
    match normalize_theme_name(name).as_str() {
        "catppuccin" | "catppuccin-mocha" | "catppuccin-latte" | "latte" | "light" => {
            ("catppuccin", "catppuccin-latte")
        }
        "tokyo-night" | "tokyonight" | "tokyo-night-day" | "tokyo-day" | "tokyonight-day" => {
            ("tokyo-night", "tokyo-night-day")
        }
        "gruvbox" | "gruvbox-dark" | "gruvbox-light" => ("gruvbox", "gruvbox-light"),
        "one-dark" | "onedark" | "one-light" | "onelight" => ("one-dark", "one-light"),
        "solarized" | "solarized-dark" | "solarized-light" => ("solarized", "solarized-light"),
        "kanagawa" | "kanagawa-lotus" | "lotus" => ("kanagawa", "kanagawa-lotus"),
        "rose-pine" | "rosepine" | "rose-pine-dawn" | "rosepine-dawn" | "dawn" => {
            ("rose-pine", "rose-pine-dawn")
        }
        _ => ("catppuccin", "catppuccin-latte"),
    }
}

fn parse_color(value: &str) -> Color {
    let value = value.trim().to_lowercase();
    if matches!(value.as_str(), "reset" | "default" | "none" | "transparent") {
        return Color::Reset;
    }
    if let Some(hex) = value.strip_prefix('#') {
        if hex.len() == 6
            && let (Ok(red), Ok(green), Ok(blue)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            )
        {
            return Color::Rgb(red, green, blue);
        }
        if hex.len() == 3 {
            let values: Vec<u8> = hex
                .chars()
                .filter_map(|character| u8::from_str_radix(&character.to_string(), 16).ok())
                .collect();
            if values.len() == 3 {
                return Color::Rgb(values[0] * 17, values[1] * 17, values[2] * 17);
            }
        }
    }
    if let Some(inner) = value
        .strip_prefix("rgb(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3
            && let (Ok(red), Ok(green), Ok(blue)) = (
                parts[0].trim().parse(),
                parts[1].trim().parse(),
                parts[2].trim().parse(),
            )
        {
            return Color::Rgb(red, green, blue);
        }
    }
    match value.as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" | "purple" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "darkgrey" => Color::DarkGray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        _ => Color::Cyan,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_gruvbox_matches_herdr_palette() {
        let palette = Palette::from_name("gruvbox").unwrap();
        assert_eq!(palette.accent, Color::Rgb(215, 153, 33));
        assert_eq!(palette.panel_bg, Color::Rgb(40, 40, 40));
        assert_eq!(palette.text, Color::Rgb(235, 219, 178));
        assert_eq!(palette.red, Color::Rgb(251, 73, 52));
    }

    #[test]
    fn config_theme_and_custom_overrides_are_applied() {
        let directory = std::env::temp_dir().join(format!(
            "herdr-deck-theme-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.toml");
        std::fs::write(
            &path,
            "[theme]\nname = \"nord\"\n[theme.custom]\naccent = \"#010203\"\npanel_bg = \"reset\"\n",
        )
        .unwrap();
        let palette = Palette::load_from(&path).unwrap();
        assert_eq!(palette.accent, Color::Rgb(1, 2, 3));
        assert_eq!(palette.panel_bg, Color::Reset);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
