use super::*;

fn parse(args: &[&str]) -> Result<CliOptions, clap::Error> {
    CliOptions::try_parse_from(std::iter::once("gaki-tui").chain(args.iter().copied()))
}

#[test]
fn no_arguments_opens_the_library() {
    let cli = parse(&[]).unwrap();
    assert_eq!(cli.chat, None);
    assert_eq!(cli.name, None);
    assert_eq!(cli.log_level, None);
}

#[test]
fn chat_takes_a_steamid_and_an_optional_name() {
    let cli = parse(&["--chat", "76561197960287930", "--name", "Gabe"]).unwrap();
    assert_eq!(cli.chat, Some(76_561_197_960_287_930));
    assert_eq!(cli.name.as_deref(), Some("Gabe"));
    assert_eq!(parse(&["--chat", "1"]).unwrap().name, None);
}

#[test]
fn chat_rejects_a_non_numeric_steamid() {
    assert!(parse(&["--chat", "gabe"]).is_err());
}

#[test]
fn name_requires_chat() {
    assert!(parse(&["--name", "Gabe"]).is_err());
}

#[test]
fn log_level_parses_its_names() {
    let cli = parse(&["--log-level", "debug"]).unwrap();
    assert_eq!(cli.log_level, Some(LogLevel::Debug));
}

#[test]
fn a_name_starting_with_a_dash_survives_the_equals_form() {
    let cli = parse(&["--chat", "1", "--name=-dash-"]).unwrap();
    assert_eq!(cli.name.as_deref(), Some("-dash-"));
}
