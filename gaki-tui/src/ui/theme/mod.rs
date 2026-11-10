//! Colours, text styles and widget looks, from the user's theme file.
//!
//! Everything visual flows through here. The defaults are Steam's dark look:
//! navy panels on a near-black canvas, a light-blue accent, soft blue borders,
//! and light blue-grey text. `theme.toml` (or `--theme <FILE>`) changes any
//! of it:
//!
//! - `[palette]`: the named colours the default styles are built from.
//! - `[styles]`: every named style, e.g. `selection = { fg = "#000000", bg =
//!   "accent", bold = true }`. A style lists only what it changes; a colour is
//!   a palette name or a hex string.
//!
//! Colours are hex: `aarrggbb` or `rrggbb` (alpha `ff`), `#` optional.
//! Terminals have no alpha, so `00` is transparent (the terminal's own
//! colour), `ff` is the colour as given, and anything between is mixed toward
//! black.
//! - `[widgets]`: the panel border type and the list selection symbol.
//!
//! The theme is read once at startup ([`install`]); a broken file refuses to
//! start, naming the problem, like the key bindings.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};
use serde::Deserialize;

#[cfg(test)]
mod tests;

/// Declares the palette: the [`Palette`] struct, its defaults, and the file's
/// `[palette]` table.
macro_rules! palette {
    ($($(#[doc = $doc:literal])* $name:ident = $r:literal, $g:literal, $b:literal;)*) => {
        /// The named colours. The default styles are built from these, and a
        /// style in the theme file can name them.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct Palette {
            $($(#[doc = $doc])* pub $name: Color,)*
        }

        impl Default for Palette {
            fn default() -> Self {
                Self { $($name: Color::Rgb($r, $g, $b),)* }
            }
        }

        #[derive(Debug, Default, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct PaletteFile {
            $($name: Option<String>,)*
        }

        impl Palette {
            /// Every palette name, in file order.
            pub const NAMES: &[&str] = &[$(stringify!($name)),*];

            /// The colour called `name`, if it's a palette name.
            pub fn named(&self, name: &str) -> Option<Color> {
                match name {
                    $(stringify!($name) => Some(self.$name),)*
                    _ => None,
                }
            }

            fn apply(&mut self, file: PaletteFile) -> Result<(), String> {
                $(if let Some(text) = file.$name {
                    self.$name = parse_color(&text, None)
                        .map_err(|e| format!("palette.{}: {e}", stringify!($name)))?;
                })*
                Ok(())
            }
        }
    };
}

palette! {
    /// Panel background (Steam "store" navy).
    bg = 0x1b, 0x28, 0x38;
    /// Canvas behind the panels (darker navy).
    bg_dark = 0x10, 0x16, 0x1d;
    /// Primary accent: Steam's light blue.
    accent = 0x66, 0xc0, 0xf4;
    /// Brighter accent for key caps, cursors and links.
    accent_bright = 0x1a, 0x9f, 0xff;
    /// Panel borders.
    border = 0x3d, 0x5a, 0x73;
    /// Body text.
    text = 0xc6, 0xd4, 0xdf;
    /// Secondary, muted text.
    text_dim = 0x76, 0x8f, 0xa3;
    /// Installed, online, success (Steam green).
    online = 0x8f, 0xd4, 0x50;
    /// Updates, warnings, failures (amber).
    warn = 0xf2, 0xc1, 0x4e;
}

/// Declares the styles: the [`Styles`] struct, each one's default (built from
/// the palette `p`), the file's `[styles]` table, and an accessor function
/// per style.
macro_rules! styles {
    ($p:ident; $($(#[doc = $doc:literal])* $name:ident = $default:expr;)*) => {
        /// Every named style.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct Styles {
            $($(#[doc = $doc])* pub $name: Style,)*
        }

        #[derive(Debug, Default, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct StylesFile {
            $($name: Option<StyleSpec>,)*
        }

        impl Styles {
            /// Every style name, in file order.
            pub const NAMES: &[&str] = &[$(stringify!($name)),*];

            /// The default styles over `palette`.
            pub fn from_palette($p: &Palette) -> Self {
                Self { $($name: $default,)* }
            }

            /// The style called `name`, if there is one.
            pub fn named(&self, name: &str) -> Option<Style> {
                match name {
                    $(stringify!($name) => Some(self.$name),)*
                    _ => None,
                }
            }

            fn apply(&mut self, file: StylesFile, palette: &Palette) -> Result<(), String> {
                $(if let Some(spec) = file.$name {
                    self.$name = spec
                        .over(self.$name, palette)
                        .map_err(|e| format!("styles.{}: {e}", stringify!($name)))?;
                })*
                Ok(())
            }
        }

        $(
            $(#[doc = $doc])*
            pub fn $name() -> Style {
                get().styles.$name
            }
        )*
    };
}

fn fg(color: Color) -> Style {
    Style::default().fg(color)
}

fn on(fg: Color, bg: Color) -> Style {
    Style::default().fg(fg).bg(bg)
}

fn bold(style: Style) -> Style {
    style.add_modifier(Modifier::BOLD)
}

styles! { p;
    /// Text on a panel.
    base = on(p.text, p.bg);
    /// The whole frame behind the panels.
    canvas = on(p.text, p.bg_dark);
    /// Panel titles.
    title = bold(on(p.accent, p.bg));
    /// Panel borders.
    border = on(p.border, p.bg);
    /// Field labels in detail tables.
    label = fg(p.accent);
    /// Field values and ordinary text.
    value = fg(p.text);
    /// Emphasised text: names, headline values.
    emphasis = bold(fg(p.text));
    /// Muted text: counts, hints, inactive items.
    dim = fg(p.text_dim);
    /// Accent text: active values, highlights.
    accent = fg(p.accent);
    /// Headings: help sections, balances, defaults.
    heading = bold(fg(p.accent));
    /// Key caps in hints.
    key = bold(fg(p.accent_bright));
    /// Links and live values (downloads, the running game).
    link = fg(p.accent_bright);
    /// Separator lines inside a panel.
    rule = fg(p.border);
    /// The text cursor in input fields.
    cursor = bold(fg(p.accent_bright));
    /// The selected row of a list or menu.
    selection = bold(on(p.bg_dark, p.accent));
    /// The active tab.
    tab_active = bold(on(p.bg_dark, p.accent));
    /// Inactive tabs.
    tab_inactive = on(p.text_dim, p.bg);
    /// Good news: installed, online, done.
    good = fg(p.online);
    /// Warnings: updates, things to look at.
    warning = fg(p.warn);
    /// Status lines under inputs (progress, errors).
    status = fg(p.warn).add_modifier(Modifier::DIM);
    /// An installed game in the library.
    item_installed = fg(p.text);
    /// A game with an update.
    item_update = fg(p.warn);
    /// A game that isn't installed.
    item_muted = fg(p.text_dim);
    /// Something switched off (a disabled DLC).
    disabled = bold(fg(p.text_dim));
    /// A failed operation.
    item_failed = bold(fg(p.warn));
    /// The border of a confirmation dialog.
    danger_border = on(p.warn, p.bg);
    /// The title of a confirmation dialog.
    danger_title = bold(on(p.warn, p.bg));
    /// The login QR code. Phones read dark modules on a light background
    /// best, so it doesn't follow the palette.
    qr = on(Color::Rgb(0, 0, 0), Color::Rgb(255, 255, 255));
}

/// Looks that aren't colours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Widgets {
    /// Panel borders.
    pub border_type: BorderType,
    /// Drawn before the selected row of the game list.
    pub highlight_symbol: String,
}

impl Default for Widgets {
    fn default() -> Self {
        Self {
            border_type: BorderType::Rounded,
            highlight_symbol: "▶ ".to_string(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct WidgetsFile {
    border_type: Option<String>,
    highlight_symbol: Option<String>,
}

impl Widgets {
    fn apply(&mut self, file: WidgetsFile) -> Result<(), String> {
        if let Some(name) = file.border_type {
            self.border_type = match name.as_str() {
                "plain" => BorderType::Plain,
                "rounded" => BorderType::Rounded,
                "double" => BorderType::Double,
                "thick" => BorderType::Thick,
                _ => {
                    return Err(format!(
                        "widgets.border_type: `{name}` isn't one of plain, rounded, double, thick"
                    ));
                }
            };
        }
        if let Some(symbol) = file.highlight_symbol {
            self.highlight_symbol = symbol;
        }
        Ok(())
    }
}

/// One style in the file. Only what's given changes; a modifier set to
/// `false` is removed.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct StyleSpec {
    fg: Option<String>,
    bg: Option<String>,
    bold: Option<bool>,
    dim: Option<bool>,
    italic: Option<bool>,
    underlined: Option<bool>,
    reversed: Option<bool>,
    crossed_out: Option<bool>,
}

impl StyleSpec {
    fn over(self, mut style: Style, palette: &Palette) -> Result<Style, String> {
        if let Some(text) = &self.fg {
            style = style.fg(parse_color(text, Some(palette)).map_err(|e| format!("fg: {e}"))?);
        }
        if let Some(text) = &self.bg {
            style = style.bg(parse_color(text, Some(palette)).map_err(|e| format!("bg: {e}"))?);
        }
        for (set, modifier) in [
            (self.bold, Modifier::BOLD),
            (self.dim, Modifier::DIM),
            (self.italic, Modifier::ITALIC),
            (self.underlined, Modifier::UNDERLINED),
            (self.reversed, Modifier::REVERSED),
            (self.crossed_out, Modifier::CROSSED_OUT),
        ] {
            style = match set {
                Some(true) => style.add_modifier(modifier),
                Some(false) => style.remove_modifier(modifier),
                None => style,
            };
        }
        Ok(style)
    }
}

/// A colour from the file: a palette name (when `palette` is given), or hex
/// `aarrggbb` / `rrggbb`, `#` optional.
fn parse_color(text: &str, palette: Option<&Palette>) -> Result<Color, String> {
    if let Some(color) = palette.and_then(|p| p.named(text)) {
        return Ok(color);
    }
    hex_color(text).ok_or_else(|| {
        let names = if palette.is_some() {
            "a palette name or "
        } else {
            ""
        };
        format!("`{text}` isn't a colour (use {names}hex `aarrggbb` or `rrggbb`)")
    })
}

/// Hex `aarrggbb` or `rrggbb` (opaque), `#` optional. Alpha `00` is the
/// terminal's own colour; a partial alpha is mixed toward black.
pub fn hex_color(text: &str) -> Option<Color> {
    let hex = text.strip_prefix('#').unwrap_or(text);
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    let argb = match hex.len() {
        6 => 0xff00_0000 | value,
        8 => value,
        _ => return None,
    };
    let [a, r, g, b] = argb.to_be_bytes();
    if a == 0 {
        return Some(Color::Reset);
    }
    let mix = |c: u8| (u16::from(c) * u16::from(a) / 255) as u8;
    Some(Color::Rgb(mix(r), mix(g), mix(b)))
}

/// The theme file as written.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    #[serde(default)]
    palette: PaletteFile,
    #[serde(default)]
    styles: StylesFile,
    #[serde(default)]
    widgets: WidgetsFile,
}

/// A whole theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub palette: Palette,
    pub styles: Styles,
    pub widgets: Widgets,
}

impl Default for Theme {
    fn default() -> Self {
        let palette = Palette::default();
        Self {
            styles: Styles::from_palette(&palette),
            palette,
            widgets: Widgets::default(),
        }
    }
}

/// A theme file that can't be used.
#[derive(Debug)]
pub struct ThemeError {
    pub path: PathBuf,
    pub message: String,
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for ThemeError {}

impl Theme {
    /// Parse a theme file's text over the defaults. The palette is read
    /// first, so the default styles follow it and the file's styles can name
    /// its colours.
    pub fn parse(text: &str) -> Result<Self, String> {
        let file: ThemeFile = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut palette = Palette::default();
        palette.apply(file.palette)?;
        let mut styles = Styles::from_palette(&palette);
        styles.apply(file.styles, &palette)?;
        let mut widgets = Widgets::default();
        widgets.apply(file.widgets)?;
        Ok(Self {
            palette,
            styles,
            widgets,
        })
    }

    /// Read the theme at `path`. A missing file is the default theme unless
    /// `required` (it was asked for by name).
    pub fn load(path: &Path, required: bool) -> Result<Self, ThemeError> {
        let error = |message: String| ThemeError {
            path: path.to_path_buf(),
            message,
        };
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).map_err(error),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => Ok(Self::default()),
            Err(e) => Err(error(e.to_string())),
        }
    }
}

static THEME: OnceLock<Theme> = OnceLock::new();
static THEME_FILE: OnceLock<PathBuf> = OnceLock::new();

/// Put `theme` in effect. `file` is the one named with `--theme`, passed on
/// to the chat windows this TUI opens. Only the first call counts; it's made
/// once at startup, before anything draws.
pub fn install(theme: Theme, file: Option<PathBuf>) {
    let _ = THEME.set(theme);
    if let Some(file) = file {
        let _ = THEME_FILE.set(file);
    }
}

/// The theme file named with `--theme`, if one was.
pub fn file() -> Option<&'static Path> {
    THEME_FILE.get().map(PathBuf::as_path)
}

/// The theme in effect: the installed one, else the default.
pub fn get() -> &'static Theme {
    THEME.get_or_init(Theme::default)
}

/// The palette in effect, for a colour that isn't a whole style.
pub fn palette() -> &'static Palette {
    &get().palette
}

/// A standard panel: the theme's border, a titled header and the panel fill.
/// The title is owned so the block is `'static`.
pub fn panel(title: String) -> Block<'static> {
    bordered(border(), Span::styled(title, self::title())).style(base())
}

/// A confirmation dialog's panel: [`panel`] in the danger colours.
pub fn danger_panel(title: String) -> Block<'static> {
    bordered(danger_border(), Span::styled(title, danger_title())).style(base())
}

/// A panel with no fill, so what's drawn under it (the artwork) shows.
pub fn transparent_panel(title: String) -> Block<'static> {
    bordered(border(), Span::styled(title, self::title()))
}

fn bordered(border: Style, title: Span<'static>) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(get().widgets.border_type)
        .border_style(border)
        .title(Line::from(title))
}
