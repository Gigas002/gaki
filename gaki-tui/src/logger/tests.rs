use std::os::unix::fs::PermissionsExt;

use super::*;

#[test]
fn log_file_and_its_directory_are_created_private() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state").join("gaki").join("gaki-tui.log");
    open_log_file(&path).unwrap();
    let mode = fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn log_file_is_appended_not_truncated() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("gaki-tui.log");
    fs::write(&path, "earlier run\n").unwrap();
    {
        use std::io::Write;
        writeln!(open_log_file(&path).unwrap(), "this run").unwrap();
    }
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "earlier run\nthis run\n"
    );
}
