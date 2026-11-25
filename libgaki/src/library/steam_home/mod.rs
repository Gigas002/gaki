//! `gaki-cli steam relocate`: the Steam client's folder keeps only the
//! client. Its library goes to gaki's `steam_library`, so gaki and Steam share
//! one library, its caches under `$XDG_CACHE_HOME/steam` and its logs under
//! `$XDG_STATE_HOME/steam`; each leaves a symlink in Steam's folder.
//!
//! The library goes in one of two layouts ([`Choice`]):
//!
//! - **Steam's**: `<library>/steamapps/`, with the games in its `common/`.
//!   Steam's `steamapps` links to it.
//! - **gaki's own**: the games directly in `<library>/`, the records
//!   (appmanifests, `compatdata/`, `workshop/`, …) in gaki's records directory
//!   for the library ([`crate::library::layout`]). Steam's `steamapps` links
//!   to the records directory, whose `common` links back to the library.
//!
//! Whatever links Steam's folders have already, from an earlier run with
//! other settings or from `sruman steam relocate`, are followed: what's there
//! moves on to where the settings say now. Planning a move to
//! [`Library::InPlace`] moves everything
//! back into Steam's folder.
//!
//! Every change is planned first ([`Plan`]), checked for clashes, and only
//! then made, one rename at a time, so a run cut short is finished by running
//! it again.

#[cfg(test)]
mod tests;

use std::{
    fmt, fs, io,
    os::unix::fs::{DirBuilderExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

/// The caches under Steam's folder, by path, and their folder under
/// `$XDG_CACHE_HOME/steam`. `steamapps/shadercache` is the library's; it
/// moves after the library does.
pub const CACHES: [(&str, &str); 6] = [
    ("appcache", "appcache"),
    ("depotcache", "depotcache"),
    ("package", "package"),
    ("config/htmlcache", "htmlcache"),
    ("config/avatarcache", "avatarcache"),
    ("steamapps/shadercache", "shadercache"),
];

/// Where Steam's `steamapps` is put together again while moving back.
const STAGING: &str = "steamapps.gaki-undo";

/// The layout the library is moved into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// `<library>/steamapps/common/<game>`.
    Steam,
    /// `<library>/<game>`, records in gaki's state directory.
    Gaki,
}

/// Where a library keeps its records and its games.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Library {
    /// In Steam's own `steamapps/`.
    InPlace,
    /// Steam's layout at this folder.
    Steam(PathBuf),
    /// gaki's own layout at this folder.
    Gaki(PathBuf),
}

impl fmt::Display for Library {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InPlace => f.write_str("in Steam's folder"),
            Self::Steam(root) => write!(f, "{} (Steam's layout)", root.display()),
            Self::Gaki(root) => write!(f, "{} (gaki's layout)", root.display()),
        }
    }
}

/// Where a library's records and games are, on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Dirs {
    records: PathBuf,
    games: PathBuf,
    /// The games folder is the library's own `common/`: everything in it is
    /// Steam's. gaki's layout shares its folder with anything else there.
    owns_games: bool,
}

/// One change to the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Creates a folder (and its parents), private when `private`.
    MakeDir { path: PathBuf, private: bool },
    /// Renames, or copies and deletes across filesystems.
    Move { from: PathBuf, to: PathBuf },
    /// Puts a symlink at `path` to `target`, replacing a symlink there.
    Link { path: PathBuf, target: PathBuf },
    /// Removes the symlink at `path`.
    Unlink { path: PathBuf },
    /// Removes the folder if it's empty, else leaves it.
    RemoveEmpty { path: PathBuf },
    /// Swaps the folder `with` in for the symlink `path` in one rename, and
    /// removes the symlink.
    Replace { path: PathBuf, with: PathBuf },
    /// Takes from the folder the permissions Steam's folder doesn't give.
    Private { path: PathBuf },
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MakeDir { path, .. } => write!(f, "create {}", path.display()),
            Self::Move { from, to } => write!(f, "move {} to {}", from.display(), to.display()),
            Self::Link { path, target } => {
                write!(f, "link {} to {}", path.display(), target.display())
            }
            Self::Unlink { path } => write!(f, "remove the link {}", path.display()),
            Self::RemoveEmpty { path } => write!(f, "remove {} if empty", path.display()),
            Self::Replace { path, with } => {
                write!(f, "put {} in place of {}", with.display(), path.display())
            }
            Self::Private { path } => write!(f, "make {} private", path.display()),
        }
    }
}

/// What a run changes, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub steps: Vec<Step>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// The folders and files that move.
    pub fn moving(&self) -> impl Iterator<Item = &Path> {
        self.steps.iter().filter_map(|step| match step {
            Step::Move { from, .. } => Some(from.as_path()),
            _ => None,
        })
    }
}

/// One cache or log folder of Steam's and where it belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// `appcache`, …, `logs`.
    pub name: &'static str,
    /// The folder under Steam's root.
    pub path: PathBuf,
    /// Where it goes.
    pub target: PathBuf,
}

/// Where a [`Place`] is now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placed {
    /// In Steam's folder.
    Here,
    /// Moved: Steam's folder links to the target.
    Moved,
    /// Not there (Steam makes it when it needs it).
    Missing,
    /// A link somewhere else.
    Elsewhere(PathBuf),
}

impl Place {
    pub fn placed(&self) -> Placed {
        let Ok(meta) = fs::symlink_metadata(&self.path) else {
            return Placed::Missing;
        };
        if !meta.file_type().is_symlink() {
            return Placed::Here;
        }
        let target = link_target(&self.path);
        if same_path(&target, &self.target) {
            Placed::Moved
        } else {
            Placed::Elsewhere(target)
        }
    }
}

/// A Steam installation and where its folders go.
#[derive(Debug, Clone)]
pub struct Host {
    /// Steam's root folder (`~/.local/share/Steam`).
    pub root: PathBuf,
    /// `$XDG_CACHE_HOME/steam`.
    pub cache: PathBuf,
    /// `$XDG_STATE_HOME/steam`.
    pub state: PathBuf,
    /// gaki's records directories (`<state dir>/libraries`).
    pub records_base: PathBuf,
}

impl Host {
    /// Steam's root, found where [`crate::library::steam_root_candidates`]
    /// look; the XDG folders and gaki's records directory from the
    /// environment.
    pub fn locate() -> Result<Self> {
        let root = crate::library::steam_root_candidates()
            .into_iter()
            .map(|root| fs::canonicalize(&root).unwrap_or(root))
            .find(|root| root.join("steam.sh").is_file() || root.join("ubuntu12_32").is_dir())
            .context("no Steam client found (looked in ~/.steam and ~/.local/share/Steam)")?;
        let home = crate::core::config::home_dir()?;
        let xdg = |var: &str, fallback: &str| {
            std::env::var_os(var)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(fallback))
        };
        Ok(Self {
            root,
            cache: xdg("XDG_CACHE_HOME", ".cache").join("steam"),
            state: xdg("XDG_STATE_HOME", ".local/state").join("steam"),
            records_base: crate::core::config::library_records_dir()?,
        })
    }

    fn steamapps(&self) -> PathBuf {
        self.root.join("steamapps")
    }

    /// Where Steam's library is now, from its `steamapps`.
    pub fn library(&self) -> Result<Library> {
        let path = self.steamapps();
        let is_link = fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
        if !is_link {
            return Ok(Library::InPlace);
        }
        let target = link_target(&path);
        if let (Some(parent), Some(name)) = (target.parent(), target.file_name())
            && same_path(parent, &self.records_base)
        {
            let name = name
                .to_str()
                .context("records directory name isn't UTF-8")?;
            return Ok(Library::Gaki(crate::library::layout::unescape(name)));
        }
        match (target.file_name(), target.parent()) {
            (Some(name), Some(root)) if name == "steamapps" => Ok(Library::Steam(root.to_owned())),
            _ => bail!(
                "{} links to {}, which is neither a Steam library's `steamapps` nor gaki's \
                 records; move it back yourself first",
                path.display(),
                target.display()
            ),
        }
    }

    /// The library in `choice`'s layout at `root`.
    pub fn target(&self, choice: Choice, root: &Path) -> Library {
        match choice {
            Choice::Steam => Library::Steam(root.to_owned()),
            Choice::Gaki => Library::Gaki(root.to_owned()),
        }
    }

    /// The layout `root` has now: gaki's when it has records, Steam's when
    /// it has a `steamapps`; `None` when it has neither.
    pub fn layout_of(&self, root: &Path) -> Option<Choice> {
        if self.records_of(root).is_dir() {
            Some(Choice::Gaki)
        } else if root.join("steamapps").is_dir() {
            Some(Choice::Steam)
        } else {
            None
        }
    }

    /// Whether Steam can use `root` in gaki's layout. Steam takes a `\` in
    /// a path for a folder separator, and a records directory's name has one
    /// for every character escaped (`-`, `.` at the start, a space, …) in the
    /// library's path: Steam would write prefixes to the wrong place.
    pub fn gaki_layout_works(&self, root: &Path) -> bool {
        !self.records_of(root).to_string_lossy().contains('\\')
    }

    fn records_of(&self, root: &Path) -> PathBuf {
        self.records_base
            .join(crate::library::layout::escape(&canonical(root)))
    }

    fn dirs(&self, library: &Library) -> Dirs {
        match library {
            Library::InPlace => Dirs {
                records: self.steamapps(),
                games: self.steamapps().join("common"),
                owns_games: true,
            },
            Library::Steam(root) => Dirs {
                records: root.join("steamapps"),
                games: root.join("steamapps/common"),
                owns_games: true,
            },
            Library::Gaki(root) => Dirs {
                records: self.records_of(root),
                games: root.clone(),
                owns_games: false,
            },
        }
    }

    /// The cache and log folders, in the order they move.
    pub fn places(&self) -> Vec<Place> {
        let mut out: Vec<Place> = CACHES
            .iter()
            .map(|(path, name)| Place {
                name,
                path: self.root.join(path),
                target: self.cache.join(name),
            })
            .collect();
        out.push(Place {
            name: "logs",
            path: self.root.join("logs"),
            target: self.state.join("logs"),
        });
        out
    }

    /// The steps that move the library from `from` to `to`. Fails, changing
    /// nothing, when something would land on something else.
    pub fn plan_library(&self, from: &Library, to: &Library) -> Result<Plan> {
        if let Library::Steam(root) | Library::Gaki(root) = to {
            let (root, steam) = (resolve(root), resolve(&self.root));
            if root.starts_with(&steam) || steam.starts_with(&root) {
                bail!(
                    "the library {} and Steam's folder {} can't be inside one another",
                    root.display(),
                    steam.display()
                );
            }
        }
        if let Library::Gaki(root) = to
            && !self.gaki_layout_works(root)
        {
            bail!(
                "Steam can't use {} in gaki's layout: its records directory {} has a `\\` in \
                 its name, which Steam reads as a folder separator. Use Steam's layout, or a \
                 library whose path has only letters, digits, `_` and `/`",
                root.display(),
                self.records_of(root).display()
            );
        }
        let src = self.dirs(from);
        let mut plan = Plan::default();
        let mut clashes = Vec::new();
        // Moving back assembles Steam's `steamapps` beside the link first.
        let dst = match to {
            Library::InPlace if from != to => Dirs {
                records: self.root.join(STAGING),
                games: self.root.join(STAGING).join("common"),
                owns_games: true,
            },
            _ => self.dirs(to),
        };

        if from == to {
            // Nothing moves; the links may still be missing, and an undo cut
            // short after its swap leaves the old link at the staging name.
            if let Library::Gaki(root) = to {
                link_common(&mut plan, &dst.records, root);
            }
            let staged = self.root.join(STAGING);
            if *to == Library::InPlace && is_link(&staged) {
                plan.steps.push(Step::Unlink { path: staged });
            }
            return Ok(plan);
        }

        let private = !matches!(to, Library::Gaki(_));
        plan.steps.push(Step::MakeDir {
            path: dst.records.clone(),
            private,
        });
        if dst.owns_games {
            plan.steps.push(Step::MakeDir {
                path: dst.games.clone(),
                private: false,
            });
        }

        // The games.
        if !same_path(&src.games, &dst.games) {
            for name in games(&src)? {
                let (from, to) = (src.games.join(&name), dst.games.join(&name));
                if exists(&to) {
                    clashes.push(format!(
                        "{} and {} both exist",
                        from.display(),
                        to.display()
                    ));
                } else {
                    plan.steps.push(Step::Move { from, to });
                }
            }
        }

        // The records, merged into what's there.
        merge(
            &src.records,
            &dst.records,
            &mut plan,
            &mut clashes,
            &|name| name == "common",
        )?;

        if !clashes.is_empty() {
            bail!(
                "nothing was moved, because these would clash:\n  {}",
                clashes.join("\n  ")
            );
        }

        // What's left behind.
        if src.owns_games {
            plan.steps.push(Step::RemoveEmpty {
                path: src.games.clone(),
            });
        } else if is_link(&src.records.join("common")) {
            plan.steps.push(Step::Unlink {
                path: src.records.join("common"),
            });
        }
        if *from != Library::InPlace {
            plan.steps.push(Step::RemoveEmpty {
                path: src.records.clone(),
            });
        }
        if let Library::Steam(old) = from
            && !matches!(to, Library::Steam(new) | Library::Gaki(new) if same_path(new, old))
        {
            plan.steps.push(Step::RemoveEmpty { path: old.clone() });
        }

        // Steam's `steamapps`, and the records' `common`.
        match to {
            Library::InPlace => plan.steps.push(Step::Replace {
                path: self.steamapps(),
                with: dst.records.clone(),
            }),
            Library::Steam(_) | Library::Gaki(_) => {
                if *from == Library::InPlace {
                    plan.steps.push(Step::RemoveEmpty {
                        path: self.steamapps(),
                    });
                }
                plan.steps.push(Step::Link {
                    path: self.steamapps(),
                    target: dst.records.clone(),
                });
                if private {
                    plan.steps.push(Step::Private {
                        path: dst.records.clone(),
                    });
                }
            }
        }
        if let Library::Gaki(root) = to {
            link_common(&mut plan, &dst.records, root);
        }
        Ok(plan)
    }

    /// The steps that move the caches and logs out (or, with `undo`, back).
    /// Like [`Host::plan_library`], fails on a clash before changing anything.
    pub fn plan_places(&self, undo: bool) -> Result<Plan> {
        let mut plan = Plan::default();
        let mut clashes = Vec::new();
        for place in self.places() {
            match (place.placed(), undo) {
                (Placed::Moved, false) | (Placed::Here | Placed::Missing, true) => {}
                (Placed::Missing, false) => {
                    plan.steps.push(Step::MakeDir {
                        path: place.target.clone(),
                        private: true,
                    });
                    link(&mut plan, &place.path, &place.target);
                }
                (Placed::Here, false) => {
                    plan.steps.push(Step::MakeDir {
                        path: place.target.clone(),
                        private: true,
                    });
                    merge(&place.path, &place.target, &mut plan, &mut clashes, &|_| {
                        false
                    })?;
                    plan.steps.push(Step::RemoveEmpty {
                        path: place.path.clone(),
                    });
                    link(&mut plan, &place.path, &place.target);
                    plan.steps.push(Step::Private {
                        path: place.target.clone(),
                    });
                }
                (Placed::Elsewhere(old), false) => {
                    plan.steps.push(Step::MakeDir {
                        path: place.target.clone(),
                        private: true,
                    });
                    if old.is_dir() {
                        merge(&old, &place.target, &mut plan, &mut clashes, &|_| false)?;
                        plan.steps.push(Step::RemoveEmpty { path: old });
                    }
                    link(&mut plan, &place.path, &place.target);
                }
                (Placed::Moved | Placed::Elsewhere(_), true) => {
                    let at = link_target(&place.path);
                    plan.steps.push(Step::Unlink {
                        path: place.path.clone(),
                    });
                    plan.steps.push(Step::MakeDir {
                        path: place.path.clone(),
                        private: false,
                    });
                    if at.is_dir() {
                        // The link is gone by then, so nothing in `path` clashes.
                        for name in entries(&at)? {
                            plan.steps.push(Step::Move {
                                from: at.join(&name),
                                to: place.path.join(&name),
                            });
                        }
                        plan.steps.push(Step::RemoveEmpty { path: at });
                    }
                }
            }
        }
        if !clashes.is_empty() {
            bail!(
                "nothing was moved, because these would clash:\n  {}",
                clashes.join("\n  ")
            );
        }
        Ok(plan)
    }

    /// Makes `plan`'s changes, calling `on_step` before each.
    pub fn execute(&self, plan: &Plan, mut on_step: impl FnMut(&Step)) -> Result<()> {
        for step in &plan.steps {
            on_step(step);
            self.apply(step)
                .with_context(|| format!("could not {step}"))?;
        }
        Ok(())
    }

    fn apply(&self, step: &Step) -> Result<()> {
        match step {
            Step::MakeDir { path, private } => {
                let mut builder = fs::DirBuilder::new();
                builder.recursive(true);
                if *private {
                    builder.mode(0o700);
                }
                builder.create(path)?;
            }
            Step::Move { from, to } => move_entry(from, to)?,
            Step::Link { path, target } => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                // A new link renamed over the old one: never no link at all.
                let staged = sibling(path, "gaki-link")?;
                remove_any(&staged)?;
                symlink(target, &staged)?;
                fs::rename(&staged, path)?;
            }
            Step::Unlink { path } => {
                if is_link(path) {
                    fs::remove_file(path)?;
                }
            }
            Step::RemoveEmpty { path } => {
                if fs::read_dir(path).is_ok_and(|mut e| e.next().is_none()) {
                    fs::remove_dir(path)?;
                }
            }
            Step::Replace { path, with } => {
                if is_link(path) {
                    rustix::fs::renameat_with(
                        rustix::fs::CWD,
                        with,
                        rustix::fs::CWD,
                        path,
                        rustix::fs::RenameFlags::EXCHANGE,
                    )?;
                    // `with` is the old link now.
                    fs::remove_file(with)?;
                } else if !exists(path) {
                    fs::rename(with, path)?;
                } else {
                    bail!("{} exists and isn't a link", path.display());
                }
            }
            Step::Private { path } => {
                let root = fs::metadata(&self.root)?;
                let denied = 0o077 & !root.permissions().mode();
                let mode = fs::metadata(path)?.permissions().mode();
                if mode & denied != 0 {
                    fs::set_permissions(path, fs::Permissions::from_mode(mode & !denied))?;
                }
            }
        }
        Ok(())
    }
}

/// The game folders to move out of `src`: everything in a library's own
/// `common/`, but only the games its appmanifests name out of a folder it
/// shares.
fn games(src: &Dirs) -> Result<Vec<String>> {
    if !src.games.is_dir() {
        return Ok(Vec::new());
    }
    if src.owns_games {
        return entries(&src.games);
    }
    let mut out = Vec::new();
    for name in entries(&src.records)? {
        if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
            continue;
        }
        let path = src.records.join(&name);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("could not read {}", path.display()))?;
        if let Some(dir) = crate::core::acf::parse_app_manifest(&text).install_dir
            && !dir.is_empty()
            && !dir.contains('/')
            && exists(&src.games.join(&dir))
            && !out.contains(&dir)
        {
            out.push(dir);
        }
    }
    out.sort();
    Ok(out)
}

/// Plans moving everything in `from` into `to`: what `to` lacks is moved,
/// folders on both sides are merged, the same symlink on both sides is
/// dropped from `from`, and anything else on both sides is a clash. Top-level
/// names `skip` holds for stay.
fn merge(
    from: &Path,
    to: &Path,
    plan: &mut Plan,
    clashes: &mut Vec<String>,
    skip: &dyn Fn(&str) -> bool,
) -> Result<()> {
    if !from.is_dir() || same_path(from, to) {
        return Ok(());
    }
    for name in entries(from)? {
        if skip(&name) {
            continue;
        }
        let (src, dst) = (from.join(&name), to.join(&name));
        let Ok(there) = fs::symlink_metadata(&dst) else {
            plan.steps.push(Step::Move { from: src, to: dst });
            continue;
        };
        let here = fs::symlink_metadata(&src)?;
        if here.file_type().is_symlink() && there.file_type().is_symlink() {
            if fs::read_link(&src)? == fs::read_link(&dst)? {
                plan.steps.push(Step::Unlink { path: src });
            } else {
                clashes.push(format!(
                    "{} and {} link to different places",
                    src.display(),
                    dst.display()
                ));
            }
        } else if here.is_dir() && there.is_dir() {
            merge(&src, &dst, plan, clashes, &|_| false)?;
            plan.steps.push(Step::RemoveEmpty { path: src });
        } else {
            clashes.push(format!(
                "{} and {} both exist",
                src.display(),
                dst.display()
            ));
        }
    }
    Ok(())
}

fn link_common(plan: &mut Plan, records: &Path, root: &Path) {
    let common = records.join("common");
    if !(is_link(&common) && same_path(&link_target(&common), root)) {
        plan.steps.push(Step::Link {
            path: common,
            target: root.to_owned(),
        });
    }
}

fn link(plan: &mut Plan, path: &Path, target: &Path) {
    plan.steps.push(Step::Link {
        path: path.to_owned(),
        target: target.to_owned(),
    });
}

/// The names in `dir`, sorted.
fn entries(dir: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("could not read {}", dir.display()))? {
        let name = entry?.file_name();
        out.push(name.into_string().map_err(|name| {
            anyhow::anyhow!("{}: a name that isn't UTF-8: {name:?}", dir.display())
        })?);
    }
    out.sort();
    Ok(out)
}

/// Renames `from` to `to`; across filesystems, copies (next to `to`, then
/// renamed into place) and deletes `from`.
pub(crate) fn move_entry(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    if exists(to) {
        bail!("{} exists", to.display());
    }
    match fs::rename(from, to) {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() != io::ErrorKind::CrossesDevices => return Err(e.into()),
        Err(_) => {}
    }
    let staged = sibling(to, "gaki-copy")?;
    remove_any(&staged)?;
    if let Err(err) = copy_any(from, &staged) {
        let _ = remove_any(&staged);
        return Err(err);
    }
    fs::rename(&staged, to)?;
    remove_any(from)?;
    Ok(())
}

/// Copies regular files (with permissions), folders and symlinks (as
/// links); refuses other file types.
fn copy_any(from: &Path, to: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(from)?;
    let kind = meta.file_type();
    if kind.is_symlink() {
        symlink(fs::read_link(from)?, to)?;
    } else if kind.is_file() {
        fs::copy(from, to)?;
    } else if kind.is_dir() {
        fs::create_dir(to)?;
        for name in entries(from)? {
            copy_any(&from.join(&name), &to.join(&name))?;
        }
        fs::set_permissions(to, fs::Permissions::from_mode(meta.permissions().mode()))?;
    } else {
        bail!("{}: not a file, folder or symlink", from.display());
    }
    Ok(())
}

/// `.<name>.<what>-<pid>` next to `path`.
fn sibling(path: &Path, what: &str) -> Result<PathBuf> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        bail!("{} isn't in a folder", path.display());
    };
    Ok(parent.join(format!(
        ".{}.{what}-{}",
        name.to_string_lossy(),
        std::process::id()
    )))
}

fn remove_any(path: &Path) -> Result<()> {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if meta.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// Where the symlink `path` leads, made absolute.
fn link_target(path: &Path) -> PathBuf {
    let target = fs::read_link(path).unwrap_or_default();
    match path.parent() {
        Some(parent) if target.is_relative() => parent.join(target),
        _ => target,
    }
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn same_path(a: &Path, b: &Path) -> bool {
    resolve(a) == resolve(b)
}

/// `path` with its symlinks resolved as far as it exists.
fn resolve(path: &Path) -> PathBuf {
    let mut existing = path;
    let mut rest = Vec::new();
    let base = loop {
        if let Ok(real) = fs::canonicalize(existing) {
            break real;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                existing = parent;
            }
            _ => break existing.to_path_buf(),
        }
    };
    rest.iter().rev().fold(base, |out, name| out.join(name))
}

/// A process of this user with a file under one of `paths` open, mapped,
/// or as its working folder or executable: its pid and name.
pub fn busy<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Option<(String, String)> {
    use std::os::unix::fs::MetadataExt;
    let dirs: Vec<PathBuf> = paths.into_iter().map(canonical).collect();
    if dirs.is_empty() {
        return None;
    }
    let under = |p: &Path| dirs.iter().any(|d| p.starts_with(d));
    let uid = rustix::process::getuid().as_raw();
    let own = fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
                && e.metadata().is_ok_and(|m| m.uid() == uid)
        });
    for entry in own {
        let proc = entry.path();
        let links = ["cwd", "exe", "root"]
            .iter()
            .map(|name| proc.join(name))
            .chain(
                fs::read_dir(proc.join("fd"))
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path()),
            );
        let mut hit = links
            .filter_map(|l| fs::read_link(l).ok())
            .any(|p| p != Path::new("/") && under(&p));
        if !hit {
            hit = fs::read_to_string(proc.join("maps")).is_ok_and(|maps| {
                maps.lines()
                    .filter_map(|line| {
                        line.split_once('/')
                            .map(|(_, rest)| Path::new("/").join(rest))
                    })
                    .any(|p| under(&p))
            });
        }
        if hit {
            let name = fs::read_to_string(proc.join("comm")).unwrap_or_default();
            return Some((
                entry.file_name().to_string_lossy().into_owned(),
                name.trim().to_owned(),
            ));
        }
    }
    None
}
