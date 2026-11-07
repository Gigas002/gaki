use ratatui::crossterm::event::{KeyEvent, KeyModifiers};

use super::*;

fn key(text: &str) -> Key {
    Key::parse(text).unwrap()
}

#[test]
fn keys_parse_and_print_back() {
    for (text, shown) in [
        ("E", "E"),
        ("e", "e"),
        ("/", "/"),
        ("+", "+"),
        ("ctrl+r", "ctrl+r"),
        ("Ctrl+R", "ctrl+R"),
        ("alt+PageDown", "alt+PageDown"),
        ("ctrl+alt+x", "ctrl+alt+x"),
        ("ctrl++", "ctrl++"),
        ("F4", "F4"),
        ("f12", "F12"),
        ("space", "Space"),
        (" ", "Space"),
        ("enter", "Enter"),
        ("Escape", "Esc"),
        ("shift+Tab", "BackTab"),
        ("BackTab", "BackTab"),
    ] {
        assert_eq!(key(text).to_string(), shown, "{text}");
    }
}

#[test]
fn bad_keys_say_why() {
    assert!(
        Key::parse("shift+e")
            .unwrap_err()
            .contains("`E`, not `shift+e`")
    );
    assert!(
        Key::parse("hyper+x")
            .unwrap_err()
            .contains("unknown modifier")
    );
    assert!(Key::parse("Enterr").unwrap_err().contains("is not a key"));
    assert!(Key::parse("F25").is_err());
    assert!(Key::parse("").is_err());
}

#[test]
fn terminal_events_normalise() {
    let event = |code, modifiers| Key::from_event(KeyEvent::new(code, modifiers));
    assert_eq!(event(KeyCode::Char('E'), KeyModifiers::SHIFT), key("E"));
    assert_eq!(event(KeyCode::Char('\n'), KeyModifiers::NONE), key("Enter"));
    assert_eq!(event(KeyCode::BackTab, KeyModifiers::SHIFT), key("BackTab"));
    assert_eq!(
        event(KeyCode::Char('r'), KeyModifiers::CONTROL),
        key("ctrl+r")
    );
    assert!(event(KeyCode::Char('c'), KeyModifiers::CONTROL).is_interrupt());
}

#[test]
fn the_defaults_parse_and_never_clash() {
    let map = Keymap::defaults();
    for &action in LibraryAction::all() {
        for &k in map.keys(action) {
            assert_eq!(map.library_action(k), Some(action), "{k}");
            assert!(!RESERVED.contains(&k), "{k} is reserved");
        }
    }
    assert_eq!(map.library_action(key("d")), Some(LibraryAction::Install));
    assert_eq!(map.label(LibraryAction::Down), "j / \u{2193}");
}

#[test]
fn action_names_are_unique_snake_case() {
    let mut names: Vec<&str> = LibraryAction::all().iter().map(|a| a.name()).collect();
    assert!(
        names
            .iter()
            .all(|n| n.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
    );
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), LibraryAction::all().len());
}

#[test]
fn an_empty_file_is_the_defaults() {
    assert_eq!(Keymap::parse("").unwrap(), Keymap::defaults());
    assert_eq!(Keymap::parse("[library]\n").unwrap(), Keymap::defaults());
}

#[test]
fn a_missing_file_is_the_defaults() {
    let map = Keymap::load(Path::new("/nonexistent/gaki/tui/keybinds.toml")).unwrap();
    assert_eq!(map, Keymap::defaults());
}

#[test]
fn a_binding_replaces_the_default_and_frees_its_key() {
    let map = Keymap::parse("[library]\ninstall = \"i\"\ndescription = \"d\"\n").unwrap();
    assert_eq!(map.library_action(key("i")), Some(LibraryAction::Install));
    assert_eq!(
        map.library_action(key("d")),
        Some(LibraryAction::Description)
    );
}

#[test]
fn a_list_binds_several_keys_and_empty_unbinds() {
    let map = Keymap::parse("[library]\nsort = [\"s\", \"ctrl+s\", \"s\"]\nhide = []\n").unwrap();
    assert_eq!(map.keys(LibraryAction::Sort), &[key("s"), key("ctrl+s")]);
    assert_eq!(map.label(LibraryAction::Sort), "s / ctrl+s");
    assert!(map.keys(LibraryAction::Hide).is_empty());
    assert_eq!(map.library_action(key("H")), None);
    assert_eq!(map.first(LibraryAction::Hide), "");
}

#[test]
fn a_clash_names_both_actions_and_the_line() {
    let e = Keymap::parse("[library]\n\nsort = \"x\"\n").unwrap_err();
    assert_eq!(e.line, Some(3));
    assert!(e.message.contains("`x` is bound to both"), "{}", e.message);
    assert!(e.message.contains("`sort`"), "{}", e.message);
    assert!(e.message.contains("`uninstall`"), "{}", e.message);
}

#[test]
fn unknown_names_and_reserved_keys_are_refused_with_the_line() {
    let e = Keymap::parse("[library]\ninstal = \"i\"\n").unwrap_err();
    assert_eq!(e.line, Some(2));
    assert!(e.message.contains("unknown action `instal`"));

    let e = Keymap::parse("[keys]\n").unwrap_err();
    assert_eq!(e.line, Some(1));
    assert!(e.message.contains("unknown section `[keys]`"));

    let e = Keymap::parse("[library]\nquit = \"Esc\"\n").unwrap_err();
    assert!(e.message.contains("`Esc` is reserved"));
    let e = Keymap::parse("[library]\nhide = \"?\"\n").unwrap_err();
    assert!(e.message.contains("reserved"));
    let e = Keymap::parse("[library]\nhide = \"ctrl+c\"\n").unwrap_err();
    assert!(e.message.contains("reserved"));

    let e = Keymap::parse("[library]\nhide = 3\n").unwrap_err();
    assert_eq!(e.line, Some(2));
    assert!(e.message.contains("expected a key or a list of keys"));

    let e = Keymap::parse("[library]\nhide = \"shift+h\"\n").unwrap_err();
    assert_eq!(e.line, Some(2));

    assert!(Keymap::parse("[library\n").unwrap_err().line.is_some());
}

#[test]
fn errors_name_the_file_and_line() {
    let e = KeymapError {
        path: Some("/c/gaki/tui/keybinds.toml".into()),
        line: Some(4),
        message: "unknown action `x` in [library]".into(),
    };
    assert_eq!(
        e.to_string(),
        "/c/gaki/tui/keybinds.toml:4: unknown action `x` in [library]"
    );
}

fn example_section<A: Actions>(table: &toml::Table) {
    let section = table
        .get(A::SECTION)
        .and_then(toml::Value::as_table)
        .unwrap_or_else(|| panic!("[{}] missing from the example", A::SECTION));
    for &action in A::all() {
        let value = section
            .get(action.name())
            .unwrap_or_else(|| panic!("{} missing from [{}]", action.name(), A::SECTION));
        assert_eq!(
            keys_of(value).unwrap(),
            action.default_keys().to_vec(),
            "[{}] {}",
            A::SECTION,
            action.name()
        );
    }
    assert_eq!(section.len(), A::all().len(), "[{}] has extras", A::SECTION);
}

/// `examples/keybinds.toml` lists every action of every section with its
/// default keys, commented out; as shipped it is exactly the defaults.
#[test]
fn the_example_file_lists_every_default() {
    let text = include_str!("../../../examples/keybinds.toml");
    assert_eq!(Keymap::parse(text).unwrap(), Keymap::defaults());
    let uncommented: String = text
        .lines()
        .map(|l| match l.strip_prefix("# ") {
            Some(rest) if is_binding_line(rest) => rest,
            _ => l,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let table: toml::Table = toml::from_str(&uncommented).unwrap();
    assert_eq!(table.len(), section_names().len());
    example_section::<LibraryAction>(&table);
    example_section::<OverlayAction>(&table);
    example_section::<InstallsAction>(&table);
    example_section::<SettingsAction>(&table);
    example_section::<WorkshopAction>(&table);
    example_section::<WorkshopBrowseAction>(&table);
    example_section::<WorkshopCommentsAction>(&table);
    example_section::<StoreAction>(&table);
    example_section::<SteamStoreAction>(&table);
    example_section::<DepotsAction>(&table);
    example_section::<DepotFilesAction>(&table);
    example_section::<GameInfoAction>(&table);
    example_section::<CloudAction>(&table);
    example_section::<CollectionsAction>(&table);
    example_section::<VersionsAction>(&table);
    example_section::<LaunchTargetsAction>(&table);
    example_section::<GameSettingsAction>(&table);
    example_section::<InstallPickerAction>(&table);
    example_section::<DlcAction>(&table);
    example_section::<BranchesAction>(&table);
    example_section::<RunningAction>(&table);
    example_section::<ProtonAction>(&table);
    example_section::<AccountAction>(&table);
    example_section::<MarketSearchAction>(&table);
    example_section::<FriendsAction>(&table);
    example_section::<UserProfileAction>(&table);
    example_section::<ActionsMenuAction>(&table);
}

/// `name = …`: a commented binding line, not prose.
fn is_binding_line(line: &str) -> bool {
    line.split_once(" = ").is_some_and(|(name, _)| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
    })
}

#[test]
fn section_names_are_unique() {
    let mut names = section_names();
    let n = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), n);
}

#[test]
fn tabs_may_share_a_key_but_one_tab_may_not() {
    let map = Keymap::defaults();
    // `r` resumes in the queue and rescans in Duplicates.
    assert_eq!(
        map.find::<InstallsAction>(key("r"), Some("queue")),
        Some(InstallsAction::Resume)
    );
    assert_eq!(
        map.find::<InstallsAction>(key("r"), Some("duplicates")),
        Some(InstallsAction::Rescan)
    );
    assert_eq!(
        map.find::<InstallsAction>(key("p"), Some("duplicates")),
        None
    );

    let e = Keymap::parse("[installs]\npause = \"f\"\n").unwrap_err();
    assert_eq!(e.line, Some(2));
    assert!(e.message.contains("`pause`"), "{}", e.message);
    assert!(e.message.contains("`forget`"), "{}", e.message);
    // Another tab's key is fine.
    assert!(Keymap::parse("[installs]\nrescan = \"p\"\n").is_ok());
}

#[test]
fn an_overlay_key_mustnt_clash_with_overlay_keys() {
    let e = Keymap::parse("[cloud]\nsync = \"j\"\n").unwrap_err();
    assert_eq!(e.line, Some(2));
    assert!(e.message.contains("`down` in [overlay]"), "{}", e.message);
    assert!(e.message.contains("`sync` in [cloud]"), "{}", e.message);

    // Moving `down` off `j` frees it for the overlay.
    let map = Keymap::parse("[overlay]\ndown = \"n\"\n[cloud]\nsync = \"j\"\n").unwrap_err();
    // …but `n` is Collections' `new`.
    assert!(
        map.message.contains("`new` in [collections]"),
        "{}",
        map.message
    );
    let map = Keymap::parse("[overlay]\ndown = \"ctrl+n\"\n[cloud]\nsync = \"j\"\n").unwrap();
    assert_eq!(
        map.find::<CloudAction>(key("j"), None),
        Some(CloudAction::Sync)
    );
    assert_eq!(map.overlay_action(key("ctrl+n")), Some(OverlayAction::Down));
}

#[test]
fn the_search_pane_takes_only_keys_that_dont_type() {
    let e = Keymap::parse("[workshop_browse]\nsubscribe = \"s\"\n").unwrap_err();
    assert!(e.message.contains("types into the text"), "{}", e.message);
    let e = Keymap::parse("[workshop_browse]\nsubscribe = \"Enter\"\n").unwrap_err();
    assert!(
        e.message.contains("is fixed in [workshop_browse]"),
        "{}",
        e.message
    );
    // The Actions menu keeps only Backspace: Enter is its `run`.
    assert!(
        Keymap::parse("[actions_menu]\nrun = [\"Enter\", \"ctrl+j\"]\nnext = \"ctrl+n\"\n").is_ok()
    );
    assert!(Keymap::parse("[actions_menu]\nnext = \"n\"\n").is_err());
    assert!(Keymap::parse("[actions_menu]\nnext = \"Backspace\"\n").is_err());
    let map = Keymap::parse("[workshop_browse]\nsubscribe = \"ctrl+s\"\n").unwrap();
    assert_eq!(
        map.find::<WorkshopBrowseAction>(key("ctrl+s"), None),
        Some(WorkshopBrowseAction::Subscribe)
    );
    // Its Tab default doesn't clash with [overlay]'s next_tab: [overlay]
    // doesn't apply while typing.
    assert!(Keymap::defaults().keys(WorkshopBrowseAction::Subscribe) == [key("Tab")]);
}

#[test]
fn hints_skip_unbound_actions() {
    assert_eq!(
        hints(&[
            ("a".into(), "add"),
            (String::new(), "gone"),
            ("Esc".into(), "close")
        ]),
        "[a] add  [Esc] close"
    );
}

#[test]
fn every_section_parses_with_its_defaults_written_out() {
    for name in section_names() {
        assert!(Keymap::parse(&format!("[{name}]\n")).is_ok(), "[{name}]");
    }
}
