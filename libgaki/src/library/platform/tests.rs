use super::*;

fn touch(path: &std::path::Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn missing_dir_is_unknown() {
    assert_eq!(detect_installed_platform("/nonexistent/gaki/test"), None);
    assert_eq!(
        detect_installed_platform_set("/nonexistent/gaki/test"),
        None
    );
}

#[test]
fn exe_means_windows() {
    let tmp = tempfile::tempdir().unwrap();
    touch(&tmp.path().join("bin/Game.EXE"), b"MZ");
    let root = tmp.path().to_str().unwrap();
    assert_eq!(detect_installed_platform(root).as_deref(), Some("windows"));
    assert_eq!(detect_installed_platform_set(root), Some((false, true)));
}

#[test]
fn elf_without_exe_means_linux() {
    let tmp = tempfile::tempdir().unwrap();
    touch(&tmp.path().join("game.x86_64"), b"\x7fELF....");
    let root = tmp.path().to_str().unwrap();
    assert_eq!(detect_installed_platform(root).as_deref(), Some("linux"));
    assert_eq!(detect_installed_platform_set(root), Some((true, false)));
}

#[test]
fn dual_depot_install_reports_both() {
    let tmp = tempfile::tempdir().unwrap();
    touch(&tmp.path().join("game"), b"\x7fELF....");
    touch(&tmp.path().join("windows/game.exe"), b"MZ");
    let root = tmp.path().to_str().unwrap();
    assert_eq!(detect_installed_platform_set(root), Some((true, true)));
}
