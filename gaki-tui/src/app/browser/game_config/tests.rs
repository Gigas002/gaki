use super::cycle;

#[test]
fn cycle_walks_the_values_then_unsets() {
    let values = ["a", "b"];
    assert_eq!(cycle(None, &values), Some("a"));
    assert_eq!(cycle(Some("a"), &values), Some("b"));
    assert_eq!(cycle(Some("b"), &values), None);
    assert_eq!(
        cycle(Some("gone"), &values),
        Some("a"),
        "a stale value restarts"
    );
    assert_eq!(cycle::<&str>(None, &[]), None);
}

#[test]
fn split_words_handles_quotes_and_escapes() {
    use super::split_words;
    assert_eq!(split_words("").unwrap(), Vec::<String>::new());
    assert_eq!(split_words("  -a   -b ").unwrap(), ["-a", "-b"]);
    assert_eq!(
        split_words(r#"-name "My Game" 'x y' a\ b """#).unwrap(),
        ["-name", "My Game", "x y", "a b", ""]
    );
    assert_eq!(split_words(r#""a\"b""#).unwrap(), [r#"a"b"#]);
    assert!(split_words("\"open").is_err());
    assert!(split_words("trailing\\").is_err());
}

#[test]
fn join_words_round_trips() {
    use super::{join_words, split_words};
    let words = ["-x", "two words", "", r#"q"uote"#, r"back\slash", "it's"];
    let joined = join_words(&words);
    assert_eq!(split_words(&joined).unwrap(), words);
    assert_eq!(join_words(&["-a", "-b"]), "-a -b");
}

#[test]
fn parse_env_reads_key_value_words() {
    use super::parse_env;
    let env = parse_env(r#"A=1 "B=two words" C="#).unwrap();
    assert_eq!(env["A"], "1");
    assert_eq!(env["B"], "two words");
    assert_eq!(env["C"], "");
    assert!(parse_env("NOEQUALS").is_err());
    assert!(parse_env("=x").is_err());
}
