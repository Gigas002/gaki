use super::*;

#[test]
fn user_dirs_parse() {
    let home = Path::new("/home/me");
    let text = "# comment\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_DOWNLOAD_DIR=\"$HOME/Загрузки\"\nXDG_PICTURES_DIR=\"/data/pics\"\n";
    assert_eq!(
        parse_user_dir(text, "DOWNLOAD", home),
        Some(PathBuf::from("/home/me/Загрузки"))
    );
    assert_eq!(
        parse_user_dir(text, "PICTURES", home),
        Some(PathBuf::from("/data/pics"))
    );
    assert_eq!(
        parse_user_dir("XDG_DOWNLOAD_DIR=\"relative\"", "DOWNLOAD", home),
        None
    );
    assert_eq!(parse_user_dir("", "DOWNLOAD", home), None);
    assert_eq!(parse_user_dir(text, "MUSIC", home), None);
}

#[test]
fn expand_dir_handles_tilde() {
    let home = Path::new("/home/me");
    assert_eq!(expand_dir("~", home), PathBuf::from("/home/me"));
    assert_eq!(expand_dir(" ~/dl ", home), PathBuf::from("/home/me/dl"));
    assert_eq!(expand_dir("/abs", home), PathBuf::from("/abs"));
    assert_eq!(expand_dir("~user/x", home), PathBuf::from("~user/x"));
}
