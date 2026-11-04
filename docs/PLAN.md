# gaki — fork plan

This fork turns **Aurelia** (a CLI Steam client, `aurelia-steam` crate) and
**Aurelia-TUI** (a TUI that drives the `aurelia` binary as a subprocess and parses
its output) into **gaki**, a lean, Linux-only Steam client. It ships as three crates
in one workspace:

| Crate      | Kind | Binary     | Role                                                    |
| ---------- | ---- | ---------- | ------------------------------------------------------- |
| `libgaki`  | lib  | —          | All logic: Steam session, library, install, cloud, launch, config/profiles |
| `gaki-cli` | bin  | `gaki-cli` | Thin CLI over `libgaki`                                 |
| `gaki-tui` | bin  | `gaki`     | Thin TUI over `libgaki` (imported from Aurelia-TUI)     |

Sources: `aurelia-questions.md` (the original upstream questions plus the request
list), `../rust-template` (ARCHITECTURE.md, CI, lint/deny/typos configs),
`../lootrs` (the profile/games config model), and `../Aurelia-TUI` (the TUI code).

Request items are referenced as **R<n>** (the numbered "Agent request" list) and
**Q<n>** (the numbered questions at the top of `aurelia-questions.md`).

---

## Goals

1. **Linux only.** No Windows code paths, no Windows-hosted Steam runtime.
2. **Official Steam required.** gaki works alongside an installed Steam client
   and doesn't replace its runtime pieces.
3. **One source of logic.** `libgaki` holds all the logic. The CLI and TUI only
   parse input and render output.
4. **Slim builds.** Each capability is a Cargo feature, so a user can build a
   client that only manages the library and cloud saves.
5. **lootrs-style profiles.** A runner, a shared prefix, env and wrappers live
   in reusable profiles. Games point at a profile. This fixes Steam's
   prefix-per-game sprawl.
6. **TOML everywhere,** with examples in `examples/` and docs in `docs/`.
7. **Template conformance.** The repo follows `docs/ARCHITECTURE.md`: CI gates,
   `mod.rs` + `tests.rs` layout, and the three-level feature matrix.

## Non-goals

- Downloading third-party runtimes (GE-Proton, proton-cachyos, Wine-TKG, umu,
  luxtorpeda). Users install these with their package manager, and gaki
  discovers them.
- Localized store or metadata. gaki keeps Steam's default (English) behavior.
- HowLongToBeat integration.
- Packaging scripts in the repo (AUR push, Nix flake, .deb metadata).

---

## Decisions to confirm

Each item has a default. The plan follows the default unless you say otherwise.

| # | Question | Default |
| - | -------- | ------- |
| D1 | **macOS.** Upstream also builds for macOS, which R1 doesn't mention. | **Drop it.** A Proton-centric client that requires official Steam is effectively Linux-only. |
| D2 | **`proto/` + `build.rs`** (R19, "propo"). `build.rs` runs pure-Rust protobuf codegen for 3 services that `steam-vent-proto` doesn't ship: `cloudconfigstore` (collections), `storequery`, and `wishlist` (store). | **Keep them, move them into `libgaki/`, and gate each proto by feature** (`CARGO_FEATURE_*` in `build.rs`). Collections is a default feature, so the codegen stays. First check whether the latest `steam-vent-proto` now ships these; if it does, delete both. |
| D3 | **`assets/`.** Only `asciiart_banner.txt` is used (`include_str!` in `main.rs`). The SVG logo is used only by the README. | **Delete `assets/`** and drop the banner. |
| D4 | **Daemon** (`aurelia daemon`). It holds one Steam session so separate CLI calls don't each log in again, which Steam rate-limits. | **Keep it in `gaki-cli`**, behind an `authentication` gate. The TUI holds its own in-process session and doesn't need it. Revisit after Phase 5. |
| D5 | **TUI framework.** Aurelia-TUI uses `tui` 0.18 (abandoned), crossterm 0.25, `atty` (RustSec advisory), and reqwest 0.11 blocking. ARCHITECTURE §7.2 forbids abandoned crates. | **Migrate to `ratatui` + current crossterm**, drop `atty`, and use `libgaki`'s async stack. |
| D6 | **Sync TUI vs async lib.** `libgaki` is tokio-async, while the TUI loop is synchronous. | The TUI owns a tokio runtime. It sends requests to `libgaki` tasks and receives results over channels, so the UI thread never blocks. |
| D7 | **Binary names.** R7 calls the TUI `gaki`, while R13 calls its crate `gaki-tui`. | Crate and directory are `gaki-tui`, and the binary is `gaki`. The CLI crate and binary are both `gaki-cli`. |
| D8 | **Store/wishlist/news/reviews/deals** commands. R24 doesn't list them. | Add a `store` feature (off). Also add `achievements` (on), unless you'd rather fold it into `library`. |

---

## Target layout

```text
gaki/
  Cargo.toml            # [workspace], [workspace.package], [workspace.dependencies]
  Cargo.lock
  deny.toml
  .typos.toml
  .github/
    dependabot.yml
    workflows/          # build, lint, test, deny, typos, doc, deploy (from rust-template)
  examples/
    config.toml
    games.toml
    profiles/
      proton.toml
      proton-system.toml
      native.toml
  docs/
    ARCHITECTURE.md
    PLAN.md
    CONFIG.md
    GAKI-CLI.md
    GAKI-TUI.md
    FEATURES.md
    FAQ.md
  libgaki/
    Cargo.toml
    build.rs            # if D2 keeps it
    proto/
    src/<module>/{mod.rs,tests.rs}
    tests/
  gaki-cli/
    Cargo.toml
    src/{main.rs,cli/,config/,settings/,logger/,app/,commands/,daemon/}
    tests/
  gaki-tui/
    Cargo.toml
    src/{main.rs,cli/,settings/,logger/,app/,ui/,utils/}
    tests/
  README.md             # short: what, install, links into docs/
  LICENSE               # GPL-3.0-or-later (unchanged)
```

---

## Feature matrix (R24)

These are defined in `libgaki`. `gaki-cli` and `gaki-tui` re-export each one
(`library = ["libgaki/library"]`) and gate their commands and views on it.

| Feature          | Default | Requires         | Covers (current code)                                          |
| ---------------- | :-----: | ---------------- | -------------------------------------------------------------- |
| *(core)*         | always  | —                | config/profiles, local library scan (ACF), proton discovery, launch pipeline |
| `authentication` | on      | —                | login (password, Steam Guard, QR), session store and crypto, keyring |
| `library`        | on      | `authentication` | owned apps, appinfo/PICS, tags, achievements, licenses         |
| `install`        | on      | `library`        | install, update, verify, uninstall, move/relink/import, DLC, backlog, downloading official Proton |
| `cloud`          | on      | `authentication` | Steam Cloud sync                                               |
| `collections`    | on      | `library`        | library collections (CloudConfigStore)                         |
| `versioning`     | off     | `install`        | branches, set-branch, downgrade, pin/unpin, manifests          |
| `depot`          | off     | `install`        | depot browser, depot listing                                   |
| `workshop`       | off     | `install`        | Workshop subscribe/download                                    |
| `social`         | off     | `authentication` | friends, chat, presence (pulls in `steam-vent-chat`)           |
| `inventory`      | off     | `authentication` | inventory, wallet                                              |
| `market`         | off     | `inventory`      | Community Market                                               |
| `store`          | off     | `authentication` | search, price, deals, news, reviews, events, wishlist, similar, players (D8) |

Optional dependencies (`steam-vent-chat`, `steam-cdn`, `qrcode`, `keyring`,
archive crates, and others) become `optional = true` and are enabled only by the
features that need them.

---

## Configuration model (R26, R29, R30, R5, Q8)

All config files live in `$XDG_CONFIG_HOME/gaki/`, and state and cache live in
`$XDG_DATA_HOME/gaki/` and `$XDG_CACHE_HOME/gaki/`. Values that hold a path
accept a leading `~`.

**`config.toml`** holds global settings:

```toml
steam_library = "~/games/steam"      # default: detected Steam library
default_profile = "proton"           # profile used when a game names none
default_platform = "windows"         # windows | linux — fixes Q5: no more --platform each time
cloud_sync = true
# proxy = "socks5://127.0.0.1:1080"
# protondb_url / github_url overrides are not needed; URLs are built in (R28)
```

**`profiles/<name>.toml`** describes one runner, one prefix, and their
environment. A prefix can be shared by any number of games.

```toml
name = "proton"
runner = "proton"                    # proton | native
proton = "experimental"              # official Steam-installed build name, or a path
                                     # e.g. "/usr/share/steam/compatibilitytools.d/proton-cachyos"
prefix = "~/prefixes/proton"         # STEAM_COMPAT_DATA_PATH; shared across games
wrappers = ["mangohud", "gamemoderun"]
[env]
PROTON_ENABLE_WAYLAND = "1"
[dll_overrides]
winhttp = "n,b"
```

**`games.toml`** is optional. A game only needs an entry when it overrides
something.

```toml
[[game]]
app_id = 292030
name = "witcher3"                    # alias usable everywhere an app id is
profile = "proton-cachyos"
platform = "windows"
args = ["-skipIntro"]
script = "~/games/run_scripts/witcher3/script.sh"   # Q8: named, any path
[game.env]
DXVK_HUD = "fps"
```

**Precedence:** CLI > `games.toml` entry > profile > `config.toml` > built-in
defaults. Each binary resolves these once, in its own `settings/` module, and
`libgaki` exposes the merge logic so both binaries resolve settings the same way.

The following settings are removed: `steam_prefix_mode`, `use_shared_compat_data`,
`windows_steam_discovery_enabled`, `steam_runtime_runner`, `steam_runtime_policy`,
`luxtorpeda_*`, `umu_*`, `language`, `game_configs` (replaced by `games.toml`),
`preferred_launch_options` (replaced by `args`), and the per-app-id `scripts/<id>.sh`
convention (replaced by `script`, with a fallback to the old path if it exists).

**Migration:** none is planned. gaki uses a fresh config directory. A one-off
`gaki-cli config import-aurelia` command can be added later if wanted.

---

## Steps

Each step is a single commit-sized change. The workspace must build and test green
at the end of every step, and from Phase 2 on all ARCHITECTURE §8 gates must pass.
I make the changes, and you review, commit, and push.

### Phase 0 — housekeeping

- [x] **0.1** Rename the branch to `master` (R0). **You do this yourself:**
  `git branch -m main master && git push -u origin master`, then switch the
  default branch on GitHub and delete `main`.
- [x] **0.2** Remove `docs/` from `.gitignore`, and add this plan.
- [x] **0.3** Remove repo cruft (R16, R17, R18): `flake.nix`, `SECURITY.md`,
  `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `scripts/aur-push.sh`,
  `.github/FUNDING.yml`, `.github/ISSUE_TEMPLATE/`, and the upstream
  `release.yml`. Also remove the `result`/`result-*` (Nix) entries from
  `.gitignore`.
- [x] **0.4** Drop `[profile.release]` and `[package.metadata.deb]` from
  `Cargo.toml` (R25). This probably also fixes the AUR LTO link error (Q2),
  because the forced `lto = "thin"` and `panic = "abort"` combination goes away.
  Recheck that error once a PKGBUILD exists.
- [x] **0.5** Add `.typos.toml`, `deny.toml`, `.github/dependabot.yml`, and the
  `build`/`lint`/`test`/`deny`/`typos`/`doc`/`deploy` workflows from
  `rust-template` (R11, R12, Q3). Adapt the workflows to a single crate for
  now. Add the typos allowlist entries that Steam/Proton jargon needs.
  *Done.* CI installs `libdbus-1-dev`, which keyring's Secret Service backend
  needs. `typos` and `cargo deny check licenses` pass. `aws-lc-sys` has an
  OpenSSL-license exception (see 7.3). `test.yml` codecov points at
  `aurelia-steam`, and `deploy.yml` already uses the final crate names; it runs
  only on tags. The `lint` and `doc` workflows stay red until step 1.8.
- [x] **0.6** Copy `rust-template/docs/ARCHITECTURE.md` to `docs/ARCHITECTURE.md`
  (R10). Add a short gaki section covering crate roles and the features table.

### Phase 1 — removal (single crate, before the restructure, so diffs stay readable)

- [x] **1.1** Drop Windows (R1): about 46 `cfg(windows)` sites across 14 files,
  the `windows-sys` dependency, the keyring `windows-native` feature, and
  `.exe` handling that only exists for Windows hosts. Windows *games* run under
  Proton and stay supported.
  *Done.* Also removed the `--windows` flag of `play` (it ran the `.exe`
  directly, which only works on a Windows host), `spawn_windows_native`, the
  Windows registry helpers, the named-pipe daemon transport, `.bat` launch
  scripts, and the `USERPROFILE` home fallback. `cfg(unix)` gates were removed,
  keeping their code, and `cfg(not(unix))` branches were deleted.
  Still deferred: `windows_steam_discovery_enabled`, which scans the *in-Wine*
  Steam, goes in 1.4. The keyring `apple-native` feature and the
  `target_os = "linux"` gates go in 1.2.
- [x] **1.2** Drop macOS (D1): the 7 `target_os = "macos"` sites.
  *Done.* Also removed every `target_os = "linux"` gate (code kept) and its
  `not(linux)` fallback. Removed the `PROTON_SUPPORTED`/`STEAM_RUNTIME_SUPPORTED`
  constants, `linux_only_msg`, `steam_runtime_unsupported_msg`, and
  `ensure_steam_runtime_supported`, together with every "only runs on Linux"
  guard and note in the commands. The JSON fields `steam_runtime_supported`,
  `supported`, and the plugins `linux` field are gone. Also removed the
  keyring `apple-native` feature and the macOS Steam root and `/Volumes`
  probes. macOS *game* builds are still recognised and skipped.
- [x] **1.3** Drop launch plugins (R2): `compat/{umu,luxtorpeda,plugin}.rs`,
  `infra/runners/luxtorpeda.rs`, the `umu`/`luxtorpeda`/`plugins` commands, and
  the related config fields and tests. Answers Q7.
  *Done.* Removed the `play --native-engine/--umu` flags, the
  `config game --native-engine/--no-native-engine/--umu/--no-umu` flags, the
  `GameRunner` enum, and the per-game `runner` field (also from `config game
  --json`). Removed the `luxtorpeda_*`/`umu_*` settings, the umu branch in the
  Wine-TKG runner, the pipeline's `force_*`/`use_umu` plumbing, and plain `.tar`
  extraction, which only umu used. Old config files that still contain these
  keys keep loading, because serde ignores unknown fields; a regression test
  covers this.
- [x] **1.4** Drop the self-contained Windows Steam runtime (R21): the
  `steam-runtime` command, `SteamRuntimePolicy`, the `steam_runtime_*` config,
  and `tests/steam_runtime_*.rs`. Treat a missing host Steam installation as a
  hard error with a clear message. Answers Q4 (see Phase 7 docs).
  *Done.* Removed the `steam-runtime` command and the master Wine Steam
  install/repair/login/stop code (`launch/mod.rs` now only declares modules),
  `config steam-runtime-runner|steam-runtime-policy`, `play --steam-mode`, and
  `config game --steam-runtime|--steam-prefix-mode`. Removed the config fields
  `steam_runtime_runner`, `steam_prefix_mode`, `use_shared_compat_data`,
  `windows_steam_discovery_enabled` and `steam_runtime_policy`, the per-game
  `use_steam_runtime`/`steam_runtime_policy`/`steam_prefix_mode`/`steam_launch_config`,
  Windows-Steam library discovery (`from_windows_steam`), and the dead
  launch-verification fields. Every Proton/Wine game now runs in its own
  `compatdata/<appid>/pfx` (`proton_compat_prefix`), and cloud sync and `stop`
  use the same path. The Wine runner has two Steam modes: `HostBridge` (when
  `--steam` is given or host Steam is running) and `Standalone` (fake-Steam
  trap). `--steam` without a host Steam install is a hard error. Launch target
  ids are now `windows-host` and `windows`. You can no longer force standalone
  while host Steam is running; that becomes a profile setting in 5.6. The
  README still describes the runtime; Phase 7 rewrites it.
- [x] **1.5** Restrict runtime downloads to official Proton (R22). Remove the
  Wine-TKG/GE downloaders (`infra/runners/wine_tkg.rs` and the related parts of
  `commands/runtimes.rs`) and the `tar`/`xz2`/`flate2` dependencies if nothing
  else uses them. Proton discovery checks, in order: system-wide
  (`/usr/share/steam/compatibilitytools.d`, then `~/.steam/root/compatibilitytools.d`),
  then Steam-installed official Proton apps. Installing official Proton
  (Experimental, or a numbered release) goes through the normal Steam app
  install path.
  *Done.* `compat/proton.rs` lost the GitHub release fetching, asset
  selection (arch/CachyOS microarch), tarball extraction and `remove`; it now
  holds the official catalogue (`official_protons`, `find_official`) and
  discovery (`search_dirs`, `list_installed`) in the order above. Every
  directory in a `compatibilitytools.d` counts, so custom builds installed by
  a distro package or by hand still run; only downloading them is gone.
  `resolve_runner` uses the same order (the Lutris and
  `<library>/compatibilitytools.d` lookups are dropped). `proton install` and
  `proton uninstall` accept only official names and go through the Steam app
  install/uninstall path; other names get an error pointing at the
  `compatibilitytools.d` dirs. Removed `tar`, `flate2` and `xz2` and the
  now-unused `byte_progress_printer`. `infra/runners/wine_tkg.rs` stays: it is
  the Proton/Wine launch runner, not a downloader (it can be renamed with the
  other identifiers in 2.4).
- [x] **1.6** Drop localized metadata (R20): `core/locale.rs`, the `--lang`
  flag, the `language`/`country` settings, and `tag_names.<lang>.json` caches.
  Keep the English defaults.
  *Done.* Deleted `core/locale.rs` (env-locale detection). Every store and
  community request now uses `core::STEAM_LANGUAGE` (`english`), so the
  `language` parameters are gone from the `steam_client` store/tag/category/
  achievement APIs, `web::store::fetch_requirements` and `ReviewQuery`.
  Removed `--lang` from `info`, `price`, `search`, `deals`, `similar`,
  `wishlist`, `achievements` and `reviews`, and the `config language` and
  `config country` commands and settings (old configs still load). The
  per-command `--country` flag stays, since it picks a price region rather
  than localizing anything; without it the region is `US`.
  `normalize_country` moved to `core::utils`. Caches are now
  `tag_names.json`, `category_names.json` and `info_cache/<appid>.<cc>.json`.
- [x] **1.7** Act on D2 and D3: delete `assets/` and the banner, and decide the
  fate of the protos. If they stay, they move in Phase 2.
  *Done.* Deleted `assets/` (banner and SVG logo). A bare `aurelia` now just
  prints the long help, and the README logo line is gone. Protos: the newest
  `steam-vent-proto` (0.5.2, checked 2026-11-04) still doesn't ship
  `storequery`, `wishlist` or `cloudconfigstore`, so `proto/` and `build.rs`
  stay and move into `libgaki/` with per-feature gating in Phase 2.
- [x] **1.8** Make the lint gates green. Fix the roughly 140 clippy findings
  (`-D warnings`, at all three feature levels) left after the removals. Most
  are collapsible `if` statements and field assignments outside initializers.
  Also fix the rustdoc `-D warnings` errors, which are public docs linking to
  private items. From here on, every ARCHITECTURE §8 gate stays green.
  *Done.* There are no cargo features yet, so "all three levels" is the
  default build until Phase 2 adds the gates. `clippy --fix` handled the
  collapsible `if`s and other mechanical lints. Field assignments became
  struct literals with `..Default::default()`. Complex types got aliases
  (`DlcNames`, `Recorded`, `TargetResolvedHook`). The two
  `mod tests { mod tests }` wrappers were flattened, and duplicate `if`
  branches were merged. Updating `async-trait` to 0.1.92 cleared the macro's
  `double_must_use`, and updating `binrw` to 0.15.2 cleared the
  future-incompat warning. Eight functions with long argument lists carry
  `#[expect(clippy::too_many_arguments, reason = ...)]` pointing at 2.3 or
  Phase 5, where they get restructured. One test holds the script-dir env
  lock across an `.await` on purpose (`#[expect(clippy::await_holding_lock)]`).
  Nine private intra-doc links became plain code spans. fmt, typos,
  `deny licenses`, clippy, rustdoc and tests all pass.
- [x] **1.9** Fix the Steam-running guard, a leftover from the Windows
  removal. `SteamClient::steam_is_running()` is hard-coded to `false`, so the
  "Steam is running, close it first" guard in `commands/common.rs` never
  fires, and `--restart-steam` (on `dlc enable|disable` and the
  relink/import commands) is a silent no-op. A running Steam client can
  therefore overwrite our appmanifest and `libraryfolders.vdf` edits. Wire
  `steam_is_running` to the existing Linux host-Steam detection. Then either
  implement stop/start (`steam -shutdown`, then a detached `steam -silent`)
  or drop `--restart-steam` and keep only the refusal.
  *Done: implemented stop/start.* `steam_is_running` uses
  `core::utils::is_steam_running` (checks `~/.steam/steam.pid`, then scans
  `/proc`). `shutdown_steam` runs `steam -shutdown` and waits up to 30s for
  the client to exit (`wait_until`, which has tests). `start_steam` spawns a
  detached `steam -silent`, sharing `start_steam_detached` with
  `ensure_steam_running`. With Steam open, `move`, `relink` and `import` now
  refuse unless given `--restart-steam`, and `dlc enable|disable` and DLC
  installs warn that a restart is needed, as they were meant to. The
  "(Windows)" wording is gone from the flag help.

### Phase 2 — rename and restructure (R3, R4, R5, R13, R14, R15)

- [x] **2.1** Convert to a workspace. The root `Cargo.toml` gets `[workspace]`,
  `[workspace.package]` (edition 2024, GPL-3.0-or-later, and repository), and
  `[workspace.dependencies]`. Create `libgaki/` from `src/lib.rs` and its
  modules (`core`, `steam_client`, `library`, `web`, `launch`, `compat`, `infra`)
  plus `proto/` and `build.rs`.
  *Done.* The root `Cargo.toml` is now a workspace (`resolver = "3"`,
  repository `Gigas002/gaki`, version 0.1.41) with every dependency in
  `[workspace.dependencies]`. Each crate lists only the crates it uses, so the
  unused `directories`, `prost`, `cbc` and `aes` were dropped. `libgaki/`
  holds the library tree, `proto/` and `build.rs`. The CLI binary stays at the
  root as package `aurelia-steam` until 2.2, and depends on
  `aurelia = { package = "libgaki", ... }` so `aurelia::` paths and
  `tests/` keep working unchanged. Three binary-only files that were pulled
  out of the library tree with `#[path]` now live in `src/` as plain modules:
  `core/output.rs` → `src/output.rs`, `compat/proc_admin.rs` →
  `src/proc_admin.rs`, and `web/steam_urls.rs` → `src/steam_urls.rs`.
  Rustdoc now builds the binary too, because it no longer shares the name
  `aurelia` with the library, so three binary doc links were fixed.
- [ ] **2.2** Create `gaki-cli/` from `src/main.rs`, `cli.rs`, `commands/`, and
  `daemon/`. Change `aurelia::` paths to `libgaki::`. Rename the binary to
  `gaki-cli`.
- [ ] **2.3** Move all logic out of `gaki-cli/src/commands/` into `libgaki`, so
  commands keep only argument mapping and output formatting (R23). Expect the
  heavy cases in `install.rs`, `info.rs`, `store.rs`, `auth.rs`, and
  `common.rs`. Output formatting (`core/output.rs`) moves to the CLI.
- [ ] **2.4** Rename identifiers: `AURELIA_*` env vars become `GAKI_*`, user-facing
  strings say gaki, the config dir moves to `gaki/`, and the user agent changes.
  Grep for `aurelia` until only acknowledgements remain.
- [ ] **2.5** Move integration tests to their owning crate's `tests/`. Create
  `examples/` (empty placeholder until Phase 5) and move documentation into
  `docs/` (leave the README content split for Phase 7). Point the codecov
  matrix in `test.yml` at the new crates.

### Phase 3 — import the TUI (R6, R7, R8, R9)

- [ ] **3.1** Copy the `../Aurelia-TUI` sources into `gaki-tui/` as they are
  (record the upstream commit `d0af998` in the commit message). Name the crate
  `gaki-tui` and the binary `gaki`. It keeps talking to the CLI as a
  subprocess for now, but invokes `gaki-cli` instead of `aurelia`.
- [ ] **3.2** Strip Windows from the TUI: the `schannel` dependency, the
  `cmd`/`open` browser branches, and Windows path code.
- [ ] **3.3** Strip HLTB: `interface/hltb.rs`, `ui/hltb.rs`,
  `tests/hltb_view.rs`, `tests/fixtures/hltb/`, `docs/HOWLONGTOBEAT.md`, and
  `scripts/test-hltb-terminal.py`. Also drop the TUI's `scripts/`, `assets/`
  screenshots, `FUNDING.yml`, and workflows. CI is unified at the workspace
  level.
- [ ] **3.4** Migrate `tui` 0.18 to `ratatui`, update crossterm, remove `atty`
  (use `std::io::IsTerminal`), and replace `tui-image-rgba-updated` with a
  maintained ratatui image widget, or drop images if none fits (D5).

### Phase 4 — TUI on `libgaki` (R23)

- [ ] **4.1** Add the async bridge (D6): an `app/` worker that owns the tokio
  runtime and a `libgaki` session, plus typed request and response channels.
- [ ] **4.2** Port `interface/aurelia.rs` (subprocess calls plus output parsing)
  to direct `libgaki` calls, one view at a time: library, details, install and
  progress, launch and running, cloud, account and login, and the rest.
- [ ] **4.3** Fix the QR login bug (Q1). Render the QR challenge from the
  `libgaki` auth flow directly, rather than scraping it from CLI output, which
  is the most likely cause of the bug. Add a render test.
- [ ] **4.4** Delete the subprocess layer and drop `reqwest` blocking and
  `serde_json` CLI parsing from the TUI.
- [ ] **4.5** Bring `gaki-tui` into line with the ARCHITECTURE layout: `cli/`,
  `settings/`, `logger/`, `app/`, one file per element in `ui/`, and `utils/`.

### Phase 5 — configuration overhaul (R26, R29, R30, R27)

- [ ] **5.1** Add a `libgaki::config` module with TOML types for `config.toml`,
  `profiles/*.toml`, and `games.toml`, and loaders that discover XDG paths. Add
  `examples/` files and a test that parses every example, as lootrs does in
  `liblootrs/tests/examples.rs`.
- [ ] **5.2** Add a `libgaki::settings` resolver that merges the layers in the
  order CLI > game > profile > config > defaults and produces a resolved
  `LaunchSettings` for each game.
- [ ] **5.3** Rework the launch pipeline (`launch/stages/resolve_profile.rs`,
  `prepare_prefix.rs`, and `build_environment.rs`) to use the resolved profile:
  the shared prefix becomes `STEAM_COMPAT_DATA_PATH`, and the pipeline adds
  wrappers, env, and DLL overrides. Steam-style per-game `compatdata/<appid>`
  is no longer created.
- [ ] **5.4** Add `default_platform` (R27/Q5). `--platform` overrides it, and
  `games.toml` can set it per game.
- [ ] **5.5** Add a named `script` path per game, with a fallback to
  `scripts/<appid>.sh` (R30/Q8), and game `name` aliases accepted wherever the
  CLI takes an app id.
- [ ] **5.6** Add a per-profile/per-game Steam integration mode (`host`,
  `none`, `auto`), replacing the dropped `--steam-mode`. `auto` keeps today's
  behaviour: bridge to host Steam when it is running or `--steam` is given.
- [ ] **5.7** CLI: rewrite the `config` subcommand for the new model and add a
  `profiles list|show` command. TUI: wire `ui/game_config.rs` to profile
  selection and per-game overrides.
- [ ] **5.8** Remove the JSON config code paths. Caches can stay JSON, because
  ARCHITECTURE requires TOML only for user configuration files. Confirm that
  this split is fine.

### Phase 6 — feature gates (R24)

- [ ] **6.1** Declare the features in `libgaki` and gate modules at their
  boundaries. Make heavy dependencies optional.
- [ ] **6.2** Forward the features from `gaki-cli` and `gaki-tui`, and gate their
  commands, views, keybindings, and help text.
- [ ] **6.3** Get the three-level matrix (`--no-default-features`, default,
  `--all-features`) green for clippy and tests, and document it in
  `docs/FEATURES.md`.

### Phase 7 — small features, docs, dependencies, tests

- [ ] **7.1** Add hotkeys that open ProtonDB (`https://www.protondb.com/app/<id>`)
  and a GitHub issue search for the app id in `ValveSoftware/Proton` (R28/Q9).
  They run `xdg-open` through `std::process::Command`, with no new crate, and
  are added to the TUI help. A CLI equivalent, `gaki-cli open protondb <app>`,
  is optional.
- [ ] **7.2** Split the README (R34) into `docs/CONFIG.md`, `docs/GAKI-CLI.md`,
  `docs/GAKI-TUI.md`, `docs/FEATURES.md`, and `docs/FAQ.md`. `FAQ.md` covers
  prerequisites: Steam must be installed, Proton must be installed
  system-wide or official (Q4). Trim the README to an overview, install
  instructions, and links, and keep the credits and acknowledgements to
  upstream.
- [ ] **7.3** Update all dependencies to their latest versions (R31) through
  `[workspace.dependencies]`. Watch the coupled pins: `steam-vent`,
  `steam-vent-proto`, `-proto-common`, `protobuf`, `rand`, and `directories`
  are matched to steam-vent on purpose. The forks `aurelia-steam-vent-chat` and
  `aurelia-steam-cdn` may need gaki-owned forks, or replacement with upstream
  crates if those have caught up. Get `cargo deny check` green, covering
  advisories as well as licenses. Known issues: `webpki-roots` (the
  distrusted emSign roots advisory), yanked `spin` and `chacha20`, duplicate
  `rand` and `getrandom` versions, and a future-incompat warning in `binrw`.
  Also replace `aws-lc-sys` (rustls' default provider, under the OpenSSL
  license, which is GPL-incompatible) with the `ring` provider, then drop the
  exception in `deny.toml`. Once `cargo deny check` passes in full, switch
  `deny.yml` from `check licenses` to `check`.
- [ ] **7.4** Refactor tests to the ARCHITECTURE layout (R32): convert the
  `*_tests.rs` siblings into `<module>/mod.rs` and `<module>/tests.rs`, and keep
  integration tests under each crate's `tests/`.
- [ ] **7.5** Raise coverage (R33). Priorities: config/settings resolution,
  profile merging, proton discovery order, launch command building, and
  feature-gated modules. Add coverage upload to `test.yml` as the template
  does.
- [ ] **7.6** Final pass: run every ARCHITECTURE §8 gate, check that `typos` is
  clean, and grep for leftover references to `aurelia`, `windows`, `umu`,
  `luxtorpeda`, `hltb`, and `locale`.

---

## Q&A carried over from `aurelia-questions.md`

| Q | Resolution |
| - | ---------- |
| Q1 QR in TUI broken | Step 4.3 |
| Q2 AUR `!lto` | Step 0.4. Re-verify when packaging. |
| Q3 CI/CD | Step 0.5 |
| Q4 Steam required? | Yes, by design (R21). Documented in step 7.2. |
| Q5 default platform | Step 5.4 |
| Q6 slim builds | Phase 6 |
| Q7 umu download / PATH | Moot. umu support is removed (step 1.3). |
| Q8 named scripts | Step 5.5 |
| Q9 ProtonDB/GitHub hotkey | Step 7.1 |
| Profiles / shared prefixes | Phase 5 |

`aurelia-questions.md` can be deleted once this plan is accepted.
