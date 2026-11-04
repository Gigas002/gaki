//! Which platform's depot is installed, judged from the files on disk.

/// Outcome of one bounded install-dir walk.
enum WalkOutcome<T> {
    /// Root missing or unreadable.
    NotDir,
    /// The visitor found its verdict.
    Found(T),
    /// Budget spent before a verdict.
    Exhausted,
    /// Every entry visited.
    Complete,
}

/// Bounded breadth-first walk over regular files.
fn walk_install_dir<T>(
    install_path: &str,
    mut visit: impl FnMut(&std::path::Path) -> Option<T>,
) -> WalkOutcome<T> {
    let root = std::path::Path::new(install_path);
    if !root.is_dir() {
        return WalkOutcome::NotDir;
    }
    let mut queue = std::collections::VecDeque::from([root.to_path_buf()]);
    let mut budget = 100_000usize;
    while let Some(dir) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if budget == 0 {
                return WalkOutcome::Exhausted;
            }
            budget -= 1;
            let path = entry.path();
            if path.is_dir() {
                queue.push_back(path);
                continue;
            }
            if let Some(found) = visit(&path) {
                return WalkOutcome::Found(found);
            }
        }
    }
    WalkOutcome::Complete
}

/// Which platform payloads exist in an install dir: (linux, windows).
/// A dual-depot install (both platforms in one dir) reports both.
/// Partial verdict at budget exhaustion.
pub fn detect_installed_platform_set(install_path: &str) -> Option<(bool, bool)> {
    let is_elf = |path: &std::path::Path| -> bool {
        use std::io::Read;
        let mut head = [0u8; 4];
        std::fs::File::open(path)
            .ok()
            .and_then(|mut f| f.read_exact(&mut head).ok())
            .is_some_and(|_| head == *b"\x7fELF")
    };
    let mut linux = false;
    let mut windows = false;
    let outcome = walk_install_dir(install_path, |path| {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        match ext.as_deref() {
            Some("exe") => windows = true,
            // ELF check only where a native binary can plausibly live.
            None | Some("so") | Some("x86_64") | Some("appimage") | Some("bin")
                if !linux && is_elf(path) =>
            {
                linux = true;
            }
            _ => {}
        }
        (linux && windows).then_some(())
    });
    match outcome {
        WalkOutcome::NotDir => None,
        _ => Some((linux, windows)),
    }
}

/// Best-effort detection of the platform whose depot is installed for a game, by
/// looking for a Windows executable in its install directory: a Windows depot
/// always ships a `.exe`, a native Linux/macOS build never does. Breadth-first so
/// a Windows game's top-level `.exe` is found immediately; the walk is bounded so
/// a large native install can't stall `list`. Returns `None` when the directory
/// can't be read or the budget is exhausted before a verdict (the caller then
/// leaves the platform unknown rather than guessing).
pub fn detect_installed_platform(install_path: &str) -> Option<String> {
    let outcome = walk_install_dir(install_path, |path| {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            .then(|| "windows".to_string())
    });
    match outcome {
        WalkOutcome::Found(platform) => Some(platform),
        WalkOutcome::Complete => Some("linux".to_string()),
        WalkOutcome::NotDir | WalkOutcome::Exhausted => None,
    }
}

#[cfg(test)]
mod tests;
