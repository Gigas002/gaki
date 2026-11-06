use clap::Parser;

use super::*;

fn merge(args: &[&str]) -> Settings {
    let cli = CliOptions::try_parse_from(std::iter::once("gaki-tui").chain(args.iter().copied()))
        .unwrap();
    Settings::merge(
        cli,
        PathBuf::from("/state/gaki-tui.log"),
        ProxyConfig::default(),
    )
}

#[test]
fn defaults_open_the_library_at_info() {
    let settings = merge(&[]);
    assert_eq!(settings.start, Start::Library);
    assert_eq!(settings.log_level, LevelFilter::INFO);
    assert_eq!(settings.log_file, PathBuf::from("/state/gaki-tui.log"));
}

#[cfg(feature = "social")]
#[test]
fn chat_starts_the_chat_view() {
    let settings = merge(&["--chat", "42", "--name", "Gabe"]);
    assert_eq!(
        settings.start,
        Start::Chat {
            steamid: 42,
            name: "Gabe".into()
        }
    );
}

#[cfg(feature = "social")]
#[test]
fn chat_without_a_name_uses_an_empty_one() {
    let settings = merge(&["--chat", "42"]);
    assert_eq!(
        settings.start,
        Start::Chat {
            steamid: 42,
            name: String::new()
        }
    );
}

#[test]
fn log_level_overrides_the_default() {
    assert_eq!(merge(&["--log-level", "off"]).log_level, LevelFilter::OFF);
    assert_eq!(
        merge(&["--log-level", "trace"]).log_level,
        LevelFilter::TRACE
    );
}
