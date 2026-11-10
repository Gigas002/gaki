use super::*;

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[test]
fn an_empty_file_is_the_default_theme() {
    assert_eq!(Theme::parse("").unwrap(), Theme::default());
}

#[test]
fn defaults_are_the_steam_look() {
    let theme = Theme::default();
    assert_eq!(theme.palette.accent, rgb(0x66, 0xc0, 0xf4));
    assert_eq!(
        theme.styles.selection,
        Style::default()
            .fg(theme.palette.bg_dark)
            .bg(theme.palette.accent)
            .add_modifier(Modifier::BOLD)
    );
    assert_eq!(theme.widgets.border_type, BorderType::Rounded);
    assert_eq!(theme.widgets.highlight_symbol, "▶ ");
}

#[test]
fn colours_are_hex_argb_or_rgb() {
    assert_eq!(hex_color("ff102030"), Some(rgb(0x10, 0x20, 0x30)));
    assert_eq!(hex_color("#102030"), Some(rgb(0x10, 0x20, 0x30)));
    assert_eq!(hex_color("#FFFFFFFF"), Some(rgb(255, 255, 255)));
    assert_eq!(hex_color("00ffffff"), Some(Color::Reset), "transparent");
    assert_eq!(
        hex_color("80ffffff"),
        Some(rgb(128, 128, 128)),
        "half alpha mixes toward black"
    );
    for bad in ["red", "fff", "1234567", "+1102030", "gg102030", "", "#"] {
        assert_eq!(hex_color(bad), None, "{bad}");
    }
}

#[test]
fn the_palette_flows_into_the_default_styles() {
    let theme = Theme::parse("[palette]\naccent = \"ff112233\"\n").unwrap();
    assert_eq!(theme.palette.accent, rgb(0x11, 0x22, 0x33));
    assert_eq!(theme.styles.accent.fg, Some(rgb(0x11, 0x22, 0x33)));
    assert_eq!(theme.styles.selection.bg, Some(rgb(0x11, 0x22, 0x33)));
    // Untouched entries keep their defaults.
    assert_eq!(theme.palette.warn, Palette::default().warn);
}

#[test]
fn a_style_changes_only_what_it_names() {
    let theme = Theme::parse(
        "[styles]\nselection = { bg = \"warn\", bold = false, underlined = true }\n\
         dim = { fg = \"#404040\" }\n",
    )
    .unwrap();
    let default = Theme::default();
    let selection = theme.styles.selection;
    assert_eq!(selection.fg, default.styles.selection.fg, "fg kept");
    assert_eq!(selection.bg, Some(default.palette.warn), "a palette name");
    assert!(!selection.add_modifier.contains(Modifier::BOLD));
    assert!(selection.add_modifier.contains(Modifier::UNDERLINED));
    assert_eq!(theme.styles.dim.fg, Some(rgb(0x40, 0x40, 0x40)));
}

#[test]
fn widgets_take_a_border_type_and_symbol() {
    let theme =
        Theme::parse("[widgets]\nborder_type = \"double\"\nhighlight_symbol = \"> \"\n").unwrap();
    assert_eq!(theme.widgets.border_type, BorderType::Double);
    assert_eq!(theme.widgets.highlight_symbol, "> ");
    let err = Theme::parse("[widgets]\nborder_type = \"wavy\"\n").unwrap_err();
    assert!(err.contains("widgets.border_type"), "{err}");
}

#[test]
fn mistakes_are_named() {
    let err = Theme::parse("[palette]\naccent = \"blue\"\n").unwrap_err();
    assert!(
        err.contains("palette.accent") && err.contains("hex"),
        "{err}"
    );
    // The palette has no palette names to refer to.
    assert!(Theme::parse("[palette]\naccent = \"warn\"\n").is_err());
    let err = Theme::parse("[styles]\nvalue = { fg = \"nope\" }\n").unwrap_err();
    assert!(err.contains("styles.value") && err.contains("fg"), "{err}");
    // Unknown names are typos, not silently ignored.
    assert!(Theme::parse("[palette]\naccnet = \"ffffff\"\n").is_err());
    assert!(Theme::parse("[styles]\nselected = { bold = true }\n").is_err());
    assert!(Theme::parse("[styles]\nvalue = { colour = \"ffffff\" }\n").is_err());
    assert!(Theme::parse("[colours]\n").is_err());
}

#[test]
fn a_missing_file_is_the_default_unless_named() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("theme.toml");
    assert_eq!(Theme::load(&path, false).unwrap(), Theme::default());
    let err = Theme::load(&path, true).unwrap_err();
    assert!(err.to_string().starts_with(&path.display().to_string()));
}

#[test]
fn a_broken_file_names_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("theme.toml");
    std::fs::write(&path, "[styles]\nkey = { fg = \"zz\" }\n").unwrap();
    let err = Theme::load(&path, false).unwrap_err().to_string();
    assert!(
        err.contains("theme.toml") && err.contains("styles.key"),
        "{err}"
    );
}

#[test]
fn the_example_theme_is_the_default() {
    let text = include_str!("../../../../examples/theme.toml");
    assert_eq!(Theme::parse(text).unwrap(), Theme::default());
}

#[test]
fn the_example_theme_lists_every_name() {
    let text = include_str!("../../../../examples/theme.toml");
    let table: toml::Table = toml::from_str(text).unwrap();
    for (section, names) in [("palette", Palette::NAMES), ("styles", Styles::NAMES)] {
        let listed = table[section].as_table().unwrap();
        for name in names {
            assert!(listed.contains_key(*name), "{section}.{name} missing");
        }
    }
    let widgets = table["widgets"].as_table().unwrap();
    assert!(widgets.contains_key("border_type") && widgets.contains_key("highlight_symbol"));
}
