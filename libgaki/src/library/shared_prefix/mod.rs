//! Shared Proton prefixes for Steam's games (experimental, part of `gaki-cli
//! steam relocate --prefix`): Steam runs a Windows game in a profile's
//! prefix instead of making a `compatdata/<app id>` for it.
//!
//! The game's `compatdata/<app id>` becomes a symlink to the profile's
//! `prefix`, and Steam's compatibility tool for the game becomes the
//! profile's Proton build: Proton converts a prefix to the build that runs
//! it, so games on different builds would convert a shared prefix back and
//! forth.
//!
//! Checked against the Steam client: uninstalling a game deletes a real
//! `compatdata/<app id>`, but leaves a symlink there, and what it points to,
//! alone; launching a game keeps the symlink; the tools survive Steam
//! quitting.
//!
//! A game's own prefix may hold its saves, so it's never deleted: it's left
//! as it is, or, when asked, moved to gaki's state directory, and moved back
//! when the game gets its own prefix again. gaki remembers the games it
//! linked (`linked.toml`, with the tool each had), and only ever unlinks
//! those. Every change is planned first ([`Plan`]) and made only after.

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::core::utils::extract_quoted_values;

/// Where things are: Steam's folder and gaki's state for shared prefixes.
pub struct Host {
    /// Steam's root folder (`~/.local/share/Steam`).
    pub root: PathBuf,
    /// `<gaki state>/shared-prefix`: moved-aside prefixes (`prefixes/<app
    /// id>`) and the games gaki linked (`linked.toml`).
    pub state: PathBuf,
}

/// A prefix to share: a profile's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shared {
    /// The profile's name.
    pub profile: String,
    /// Its `prefix`, `~` expanded.
    pub prefix: PathBuf,
    /// Steam's name for its Proton build (`proton-cachyos-native`), set as
    /// the game's compatibility tool; `None` leaves Steam's choice alone.
    pub tool: Option<String>,
}

/// Which prefix a game should have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Want {
    Shared(Shared),
    /// Its own `compatdata/<app id>`: a shared one is unlinked, and a prefix
    /// moved aside comes back.
    Own,
}

/// An installed game Steam runs through Proton.
#[derive(Debug, Clone)]
pub struct Game {
    pub app_id: u32,
    pub name: String,
    /// `<library>/steamapps/compatdata/<app id>`.
    pub compat: PathBuf,
    /// Its compatibility tool in Steam's `config.vdf`, if set.
    pub tool: Option<String>,
}

/// What a game's `compatdata/<app id>` is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prefix {
    /// Nothing yet: Steam makes it on the first launch.
    None,
    /// The game's own prefix, a directory.
    Own,
    /// A symlink to `target`; `ours` when gaki made it.
    Linked { target: PathBuf, ours: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Create a shared prefix's directory.
    MakeDir(PathBuf),
    /// Move a game's own prefix out of the way, or back.
    Move {
        from: PathBuf,
        to: PathBuf,
    },
    Link {
        app_id: u32,
        path: PathBuf,
        target: PathBuf,
    },
    Unlink {
        app_id: u32,
        path: PathBuf,
    },
    /// Set a game's compatibility tool in `config.vdf`, or (`None`) unset it.
    Tool {
        app_id: u32,
        name: Option<String>,
    },
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MakeDir(path) => write!(f, "create {}", path.display()),
            Self::Move { from, to } => write!(f, "move {} to {}", from.display(), to.display()),
            Self::Link { path, target, .. } => {
                write!(f, "link {} to {}", path.display(), target.display())
            }
            Self::Unlink { path, .. } => write!(f, "remove the link {}", path.display()),
            Self::Tool {
                app_id,
                name: Some(name),
            } => write!(f, "set app {app_id}'s compatibility tool to {name}"),
            Self::Tool { app_id, name: None } => {
                write!(f, "unset app {app_id}'s compatibility tool")
            }
        }
    }
}

/// A game left as it is, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kept {
    /// It has its own prefix (and `replace` wasn't asked).
    Own(PathBuf),
    /// Its `compatdata/<app id>` is a link gaki didn't make.
    Elsewhere(PathBuf),
}

#[derive(Debug, Default)]
pub struct Plan {
    pub steps: Vec<Step>,
    /// Games left alone: their name and why.
    pub kept: Vec<(String, Kept)>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// The own prefixes this plan moves.
    pub fn moving(&self) -> impl Iterator<Item = &Path> {
        self.steps.iter().filter_map(|step| match step {
            Step::Move { from, .. } => Some(from.as_path()),
            _ => None,
        })
    }
}

impl Host {
    /// Steam's root (as `steam relocate` finds it) and gaki's state
    /// directory.
    pub fn locate() -> Result<Self> {
        Ok(Self {
            root: crate::library::steam_home::Host::locate()?.root,
            state: crate::core::config::shared_prefix_dir()?,
        })
    }

    fn config_vdf(&self) -> PathBuf {
        self.root.join("config/config.vdf")
    }

    fn aside(&self, app_id: u32) -> PathBuf {
        self.state.join("prefixes").join(app_id.to_string())
    }

    fn linked_file(&self) -> PathBuf {
        self.state.join("linked.toml")
    }

    /// The compatibility tools in Steam's `config.vdf`.
    pub fn tool_mapping(&self) -> Result<BTreeMap<u32, String>> {
        match fs::read_to_string(self.config_vdf()) {
            Ok(text) => Ok(tool_mapping(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(e).with_context(|| format!("reading {}", self.config_vdf().display())),
        }
    }

    /// The installed games of Steam's libraries that Steam runs through
    /// Proton: the ones with a compatibility tool set, and the ones whose
    /// install has Windows executables and no Linux ones. Steam's own tools
    /// (runtimes, Proton, redistributables) aren't games.
    pub fn games(&self) -> Result<Vec<Game>> {
        let tools = self.tool_mapping()?;
        let folders = self.root.join("steamapps/libraryfolders.vdf");
        let mut libraries = vec![self.root.clone()];
        if let Ok(text) = fs::read_to_string(&folders) {
            for library in crate::library::library_folder_paths(&text) {
                if !libraries.iter().any(|known| same_path(known, &library)) {
                    libraries.push(library);
                }
            }
        }
        let mut games = Vec::new();
        for library in libraries {
            let steamapps = library.join("steamapps");
            for entry in fs::read_dir(&steamapps).into_iter().flatten().flatten() {
                let file = entry.file_name();
                let file = file.to_string_lossy();
                let Some(app_id) = file
                    .strip_prefix("appmanifest_")
                    .and_then(|rest| rest.strip_suffix(".acf"))
                    .and_then(|id| id.parse::<u32>().ok())
                else {
                    continue;
                };
                let Ok(raw) = fs::read_to_string(entry.path()) else {
                    continue;
                };
                let manifest = crate::core::acf::parse_app_manifest(&raw);
                let name = manifest
                    .display_name()
                    .unwrap_or_else(|| app_id.to_string());
                if crate::library::is_ignored_steam_app(app_id, &name)
                    || games.iter().any(|game: &Game| game.app_id == app_id)
                {
                    continue;
                }
                let tool = tools.get(&app_id).cloned();
                let proton = match &tool {
                    Some(tool) => !tool.to_ascii_lowercase().starts_with("steamlinuxruntime"),
                    None => manifest.install_dir.as_deref().is_some_and(|dir| {
                        let path = steamapps.join("common").join(dir);
                        crate::library::platform::detect_installed_platform_set(
                            &path.to_string_lossy(),
                        ) == Some((false, true))
                    }),
                };
                if proton {
                    games.push(Game {
                        app_id,
                        name,
                        compat: steamapps.join("compatdata").join(app_id.to_string()),
                        tool,
                    });
                }
            }
        }
        games.sort_by_key(|game| game.app_id);
        Ok(games)
    }

    /// What `game`'s `compatdata/<app id>` is now.
    pub fn prefix(&self, game: &Game) -> Result<Prefix> {
        Ok(match fs::symlink_metadata(&game.compat) {
            Err(_) => Prefix::None,
            Ok(meta) if meta.file_type().is_symlink() => Prefix::Linked {
                target: link_target(&game.compat),
                ours: self.linked()?.contains_key(&game.app_id),
            },
            Ok(_) => Prefix::Own,
        })
    }

    /// Give each game the prefix `want` says (`None`: leave it). With
    /// `replace`, a game's own prefix moves aside for a shared one; without,
    /// the game is kept as it is.
    pub fn plan(
        &self,
        games: &[Game],
        want: impl Fn(&Game) -> Option<Want>,
        replace: bool,
    ) -> Result<Plan> {
        let linked = self.linked()?;
        let mut plan = Plan::default();
        let mut made = BTreeSet::new();
        for game in games {
            let Some(want) = want(game) else {
                continue;
            };
            let prefix = self.prefix(game)?;
            let shared = match want {
                Want::Own => {
                    if let Prefix::Linked { ours: true, .. } = prefix {
                        self.unshare(game, &linked, &mut plan);
                    }
                    continue;
                }
                Want::Shared(shared) => shared,
            };
            if !shared.prefix.is_absolute() {
                bail!(
                    "profile `{}`'s prefix {} isn't an absolute path",
                    shared.profile,
                    shared.prefix.display()
                );
            }
            if fs::symlink_metadata(&shared.prefix).is_err() {
                if made.insert(shared.prefix.clone()) {
                    plan.steps.push(Step::MakeDir(shared.prefix.clone()));
                }
            } else if !shared.prefix.is_dir() {
                bail!("{} isn't a directory", shared.prefix.display());
            }
            let link = Step::Link {
                app_id: game.app_id,
                path: game.compat.clone(),
                target: shared.prefix.clone(),
            };
            match prefix {
                Prefix::Linked { target, .. } if same_path(&target, &shared.prefix) => {}
                Prefix::None => plan.steps.push(link),
                Prefix::Own if replace => {
                    let aside = self.aside(game.app_id);
                    if fs::symlink_metadata(&aside).is_ok() {
                        bail!(
                            "{} already holds a prefix moved aside for app {}; move it \
                             yourself first",
                            aside.display(),
                            game.app_id
                        );
                    }
                    if same_path(&game.compat, &shared.prefix) {
                        bail!(
                            "{} is profile `{}`'s prefix itself; pick another prefix",
                            game.compat.display(),
                            shared.profile
                        );
                    }
                    plan.steps.push(Step::Move {
                        from: game.compat.clone(),
                        to: aside,
                    });
                    plan.steps.push(link);
                }
                Prefix::Own => {
                    plan.kept
                        .push((game.name.clone(), Kept::Own(game.compat.clone())));
                    continue;
                }
                // Linked by gaki to another profile's prefix: relink.
                Prefix::Linked { ours: true, .. } => {
                    plan.steps.push(Step::Unlink {
                        app_id: game.app_id,
                        path: game.compat.clone(),
                    });
                    plan.steps.push(link);
                }
                Prefix::Linked {
                    target,
                    ours: false,
                } => {
                    plan.kept.push((game.name.clone(), Kept::Elsewhere(target)));
                    continue;
                }
            }
            if let Some(tool) = &shared.tool
                && game.tool.as_deref() != Some(tool.as_str())
            {
                plan.steps.push(Step::Tool {
                    app_id: game.app_id,
                    name: Some(tool.clone()),
                });
            }
        }
        Ok(plan)
    }

    /// Steps that give a game gaki linked its own prefix back.
    fn unshare(&self, game: &Game, linked: &BTreeMap<u32, String>, plan: &mut Plan) {
        plan.steps.push(Step::Unlink {
            app_id: game.app_id,
            path: game.compat.clone(),
        });
        let aside = self.aside(game.app_id);
        if fs::symlink_metadata(&aside).is_ok() {
            plan.steps.push(Step::Move {
                from: aside,
                to: game.compat.clone(),
            });
        }
        if let Some(before) = linked.get(&game.app_id) {
            let before = (!before.is_empty()).then(|| before.clone());
            if game.tool != before {
                plan.steps.push(Step::Tool {
                    app_id: game.app_id,
                    name: before,
                });
            }
        }
    }

    /// The games gaki linked, with the tool each had before: app id → tool,
    /// `""` for none.
    pub fn linked(&self) -> Result<BTreeMap<u32, String>> {
        let path = self.linked_file();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        let raw: BTreeMap<String, String> =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        Ok(raw
            .into_iter()
            .filter_map(|(id, tool)| Some((id.parse().ok()?, tool)))
            .collect())
    }

    fn save_linked(&self, linked: &BTreeMap<u32, String>) -> Result<()> {
        let path = self.linked_file();
        if linked.is_empty() {
            return match fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
                _ => Ok(()),
            };
        }
        let raw: BTreeMap<String, &String> = linked
            .iter()
            .map(|(id, tool)| (id.to_string(), tool))
            .collect();
        fs::create_dir_all(&self.state)?;
        crate::config::edit::write_atomic(&path, &toml::to_string(&raw)?)
    }

    /// Make `plan`'s changes. Steam must not be running: it rewrites
    /// `config.vdf` when it quits.
    pub fn execute(&self, plan: &Plan, mut on_step: impl FnMut(&Step)) -> Result<()> {
        let current = self.tool_mapping()?;
        let mut linked = self.linked()?;
        let mut tools = BTreeMap::new();
        for step in &plan.steps {
            on_step(step);
            match step {
                Step::MakeDir(path) => fs::create_dir_all(path)
                    .with_context(|| format!("creating {}", path.display()))?,
                Step::Move { from, to } => {
                    if let Some(parent) = to.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    crate::library::steam_home::move_entry(from, to).with_context(|| {
                        format!("moving {} to {}", from.display(), to.display())
                    })?;
                }
                Step::Link {
                    app_id,
                    path,
                    target,
                } => {
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    symlink(target, path).with_context(|| format!("linking {}", path.display()))?;
                    // Remember the first tool the game had.
                    linked
                        .entry(*app_id)
                        .or_insert_with(|| current.get(app_id).cloned().unwrap_or_default());
                    self.save_linked(&linked)?;
                }
                Step::Unlink { app_id, path } => {
                    // Only ever the link itself, never what it points to.
                    let meta = fs::symlink_metadata(path)?;
                    if !meta.file_type().is_symlink() {
                        bail!("{} isn't a symlink any more; leaving it", path.display());
                    }
                    fs::remove_file(path)
                        .with_context(|| format!("removing the link {}", path.display()))?;
                    let relinked = plan.steps.iter().any(
                        |other| matches!(other, Step::Link { app_id: id, .. } if id == app_id),
                    );
                    if !relinked {
                        linked.remove(app_id);
                    }
                }
                Step::Tool { app_id, name } => {
                    tools.insert(*app_id, name.clone());
                }
            }
        }
        // Tools go last, and the records with them: a run cut short before
        // here leaves the records naming every link, so a rerun finishes.
        if !tools.is_empty() {
            let path = self.config_vdf();
            let text =
                fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            let edited = set_tool_mapping(&text, &tools)?;
            let backup = path.with_extension("vdf.gaki-backup");
            fs::copy(&path, &backup).with_context(|| format!("backing up {}", path.display()))?;
            crate::config::edit::write_atomic(&path, &edited)?;
        }
        self.save_linked(&linked)?;
        // Nothing left shared: gaki's state for it goes.
        let _ = fs::remove_dir(self.state.join("prefixes"));
        let _ = fs::remove_dir(&self.state);
        Ok(())
    }
}

/// Delete a game's `compatdata/<app id>`: its own prefix, or, when it links
/// to a shared one, only the link.
pub fn remove_compat(compat: &Path) -> Result<()> {
    match fs::symlink_metadata(compat) {
        Ok(meta) if meta.file_type().is_symlink() => fs::remove_file(compat)
            .with_context(|| format!("failed removing the link {}", compat.display())),
        Ok(_) => fs::remove_dir_all(compat)
            .with_context(|| format!("failed deleting {}", compat.display())),
        Err(_) => Ok(()),
    }
}

/// Steam's name for the Proton build in `dir`: the tool its
/// `compatibilitytool.vdf` declares, or `proton_experimental` for Steam's
/// Proton Experimental. `None` when it can't tell.
pub fn steam_tool_name(dir: &Path) -> Option<String> {
    if let Ok(text) = fs::read_to_string(dir.join("compatibilitytool.vdf")) {
        let mut in_tools = false;
        for line in text.lines() {
            let tokens = extract_quoted_values(line.trim());
            match tokens.as_slice() {
                [key] if key.eq_ignore_ascii_case("compat_tools") => in_tools = true,
                [name] if in_tools => return Some(name.clone()),
                _ => {}
            }
        }
        return None;
    }
    match dir.file_name()?.to_str()? {
        "Proton - Experimental" => Some("proton_experimental".to_owned()),
        _ => None,
    }
}

const MAPPING_PATH: [&str; 5] = [
    "installconfigstore",
    "software",
    "valve",
    "steam",
    "compattoolmapping",
];

/// The lines of a text VDF file, each with the path of keys of the block it's
/// in.
fn walk(vdf: &str) -> Vec<(Vec<String>, &str)> {
    let mut stack: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;
    let mut out = Vec::new();
    for line in vdf.split_inclusive('\n') {
        let trimmed = line.trim();
        out.push((stack.clone(), line));
        if trimmed == "{" {
            stack.push(pending.take().unwrap_or_default());
        } else if trimmed == "}" {
            stack.pop();
            pending = None;
        } else {
            let tokens = extract_quoted_values(trimmed);
            pending = (tokens.len() == 1).then(|| tokens[0].to_ascii_lowercase());
        }
    }
    out
}

fn is_mapping(stack: &[String]) -> bool {
    stack.len() == MAPPING_PATH.len() && stack.iter().zip(MAPPING_PATH).all(|(a, b)| a == b)
}

/// `CompatToolMapping` of a text `config.vdf`: app id → tool name. App id 0
/// is Steam's default for every game, not a game.
pub fn tool_mapping(vdf: &str) -> BTreeMap<u32, String> {
    let mut map = BTreeMap::new();
    for (stack, line) in walk(vdf) {
        if stack.len() == MAPPING_PATH.len() + 1
            && is_mapping(&stack[..MAPPING_PATH.len()])
            && let Ok(app_id) = stack[MAPPING_PATH.len()].parse::<u32>()
            && app_id != 0
            && let [key, value] = extract_quoted_values(line.trim()).as_slice()
            && key.eq_ignore_ascii_case("name")
        {
            map.insert(app_id, value.clone());
        }
    }
    map
}

/// `vdf` with the `CompatToolMapping` entries of `changes` set (`Some`) or
/// removed (`None`). Every other line is copied through unchanged. Adds the
/// `CompatToolMapping` block to Steam's settings when there's none.
pub fn set_tool_mapping(vdf: &str, changes: &BTreeMap<u32, Option<String>>) -> Result<String> {
    let entry = |indent: &str, app_id: u32, name: &str| {
        format!(
            "{indent}\"{app_id}\"\n{indent}{{\n{indent}\t\"name\"\t\t\"{name}\"\n\
             {indent}\t\"config\"\t\t\"\"\n{indent}\t\"priority\"\t\t\"250\"\n{indent}}}\n"
        )
    };
    let entries = |indent: &str| -> String {
        changes
            .iter()
            .filter_map(|(app_id, name)| Some(entry(indent, *app_id, name.as_deref()?)))
            .collect()
    };
    let lines = walk(vdf);
    let has_mapping = lines.iter().any(|(stack, _)| is_mapping(stack));
    let mut out = String::with_capacity(vdf.len() + 256);
    let mut done = false;
    // The block of a changed app being skipped: its depth.
    let mut skipping: Option<usize> = None;
    for (i, (stack, line)) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some(depth) = skipping {
            if trimmed == "}" && stack.len() == depth + 1 {
                skipping = None;
            }
            continue;
        }
        if is_mapping(stack) {
            if let [key] = extract_quoted_values(trimmed).as_slice()
                && let Ok(app_id) = key.parse::<u32>()
                && changes.contains_key(&app_id)
                && lines.get(i + 1).is_some_and(|(_, next)| next.trim() == "{")
            {
                skipping = Some(stack.len());
                continue;
            }
            if trimmed == "}" {
                let indent = &line[..line.len() - line.trim_start().len()];
                out.push_str(&entries(&format!("{indent}\t")));
                done = true;
            }
        } else if !has_mapping
            && trimmed == "}"
            && stack.len() == MAPPING_PATH.len() - 1
            && stack.iter().zip(MAPPING_PATH).all(|(a, b)| a == b)
        {
            let indent = format!("{}\t", &line[..line.len() - line.trim_start().len()]);
            out.push_str(&format!(
                "{indent}\"CompatToolMapping\"\n{indent}{{\n{}{indent}}}\n",
                entries(&format!("{indent}\t"))
            ));
            done = true;
        }
        out.push_str(line);
    }
    if !done {
        bail!("config.vdf has no Steam settings block to add compatibility tools to");
    }
    Ok(out)
}

/// Where the symlink `path` leads, made absolute.
fn link_target(path: &Path) -> PathBuf {
    let target = fs::read_link(path).unwrap_or_default();
    match path.parent() {
        Some(parent) if target.is_relative() => parent.join(target),
        _ => target,
    }
}

/// Whether `a` and `b` are the same directory, through symlinks.
pub fn same_path(a: &Path, b: &Path) -> bool {
    let real = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    real(a) == real(b)
}
