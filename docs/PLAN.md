# gaki — fork plan

This fork turns **Aurelia** (a CLI Steam client, `aurelia-steam` crate) and
**Aurelia-TUI** (a TUI that drives the `aurelia` binary as a subprocess and parses
its output) into **gaki**, a lean, Linux-only Steam client. It ships as three crates
in one workspace:

| Crate      | Kind | Binary     | Role                                                    |
| ---------- | ---- | ---------- | ------------------------------------------------------- |
| `libgaki`  | lib  | —          | All logic: Steam session, library, install, cloud, launch, config/profiles |
| `gaki-cli` | bin  | `gaki-cli` | Thin CLI over `libgaki`                                 |
| `gaki-tui` | bin  | `gaki-tui` | Thin TUI over `libgaki` (imported from Aurelia-TUI)     |

Sources: `docs/aurelia-questions.md` (the original upstream questions plus the request
list), `../rust-template` (ARCHITECTURE.md, CI, lint/deny/typos configs),
`../lootrs` (the profile/games config model), and `../Aurelia-TUI` (the TUI code).

Request items are referenced as **R<n>** (the numbered "Agent request" list) and
**Q<n>** (the numbered questions at the top of `docs/aurelia-questions.md`).

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
- HowLongToBeat integration, including a link hotkey (see 7.1).
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
| D7 | **Binary names.** R7 calls the TUI `gaki`, while R13 calls its crate `gaki-tui`. | **Decided (plans2):** crate, directory and binary are all `gaki-tui` (step 4.6). The CLI crate and binary are both `gaki-cli`. |
| D8 | **Store/wishlist/news/reviews/deals** commands. R24 doesn't list them. | Add a `store` feature (off). Also add `achievements` (on), unless you'd rather fold it into `library`. |
| D9 | **Reuse `liblootrs` profiles?** Depending on `liblootrs` would let lootrs and gaki share profiles, and later let lootrs launch Steam games with the same settings. | **Don't depend on `liblootrs` now. Share the file format instead.** The two launch models differ: lootrs runs an `exe` through wine, umu or native, while gaki runs a Steam `app_id` through Steam's own `proton run` with `STEAM_COMPAT_*` and the host Steam bridge. Their game entries differ too (`exe` vs `app_id`). `liblootrs` also carries the umu runner, which gaki dropped (R2), and it's an unpublished 0.1 crate whose releases would then gate gaki's. So gaki's `Profile` uses lootrs' field names and semantics (`name`, `runner`, `prefix`, `runner_args`, `wrappers`, `debug`, `env`, `dll_overrides`, `[anticheat]`) and adds `runner = "proton"` with a `[proton]` section. A `profiles_dir` key in `config.toml` can point at `~/.config/lootrs/profiles`, so one set of profile files serves both tools. Each tool keeps its own `games.toml` and credentials (lootrs never sees Steam auth). If the overlap grows, extract the model and validation (`Profile`, `Issue`, `validate_name`, `~` expansion) into a small crate that both depend on (step 5.1). |
| D10 | **Credential storage.** See "Authentication and session security" below. | **The refresh token lives only in the OS keyring, or only in memory.** It's never in a file and never stored as plaintext. Re-authenticating for every action isn't the default (see that section for why). Sensitive actions can opt into a fresh login. |

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
  LICENSE               # plain GPLv3 text only (step 0.7)
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
environment. A prefix can be shared by any number of games. The schema follows
lootrs' `Profile` field for field (D9), so a lootrs `native` profile loads
unchanged, and `profiles_dir` in `config.toml` can point at lootrs' directory.
gaki reports runners it doesn't support (`wine`, `umu`) as validation issues,
the same way lootrs reports a runner left out of its build.

```toml
name = "proton"
runner = "proton"                    # proton | native
prefix = "~/prefixes/proton"         # STEAM_COMPAT_DATA_PATH; shared across games
wrappers = ["mangohud", "gamemoderun"]
[proton]
build = "experimental"               # official Steam-installed build name, or a path
                                     # e.g. "/usr/share/steam/compatibilitytools.d/proton-cachyos"
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
wrappers = ["~/games/run_scripts/witcher3/script.sh"]  # Q8: any path; inside the profile's
[game.env]
DXVK_HUD = "fps"
```

**Precedence:** CLI > `games.toml` entry > profile > `config.toml` > built-in
defaults. Each binary resolves these once, in its own `settings/` module, and
`libgaki` exposes the merge logic so both binaries resolve settings the same way.

The following settings are removed: `steam_prefix_mode`, `use_shared_compat_data`,
`windows_steam_discovery_enabled`, `steam_runtime_runner`, `steam_runtime_policy`,
`luxtorpeda_*`, `umu_*`, `language`, `game_configs` (replaced by `games.toml`),
`preferred_launch_options` (replaced by `args`), and launch scripts: both the
per-app-id `scripts/<id>.sh` convention and the named `script` (5.5) are
replaced by per-game `wrappers` (5.7c); a script is just a wrapper.

**Migration:** none is planned. gaki uses a fresh config directory. A one-off
`gaki-cli config import-aurelia` command can be added later if wanted.

---

## Authentication and session security (D10)

### Current state (checked 2026-11-05)

- **What's stored.** `~/.config/gaki/session.json` holds the Steam
  **refresh token** (long-lived, and enough to act as the account), plus the
  optional web access token, the account name and the SteamID.
- **Encryption is opt-in by accident.** `encrypt_session` defaults to `false`.
  The first successful login turns it on (`ops::auth::secure_session`). If
  the keyring is unreachable, the token is written **in plaintext**. The file
  is `chmod 0600`'d only after a non-atomic write, so for a moment it has
  umask permissions.
- **Wrong directory.** The file sits in the config dir, which people commonly
  sync or commit with their dotfiles.
- **The envelope adds little.** It's ChaCha20-Poly1305 with a random key kept
  in the Secret Service (`gaki`/`session-key`), run through Argon2, which does
  nothing for a random key. A legacy password-keyed mode still exists.
- **Who shares it.** `gaki-cli`, its daemon and `gaki-tui` read the same file
  and keyring entry. Launched games and scripts get no token (the `GAKI_*`
  variables carry none).
- **The daemon holds a live session** behind a Unix socket. There's no peer
  check (`SO_PEERCRED`), the socket gets umask permissions, and it falls back
  to `/tmp` when `XDG_RUNTIME_DIR` is unset.
- **Logout is local only.** It deletes the file but doesn't revoke the
  refresh token at Steam, so a copied token keeps working.

**Threat model, honestly:** on Linux the Secret Service has no per-app access
control. Any process running as your user, including a game under Proton, can
read an unlocked keyring item, connect to the daemon socket, or read gaki's
memory. What gaki can defend against: token leaks through files (backups,
dotfiles, other users), a token that outlives logout, tokens in logs, and
other local users.

### Why not re-authenticate on every action

Each Steam login takes the password plus a Steam Guard code or mobile approval,
and Steam rate-limits logins (a rate limit is also why the daemon exists). Asking
for them on every command would make gaki unusable. It would also expose the
**password**, which can't be revoked and is used everywhere, far more often than
a refresh token, which is scoped and revocable. The safer equivalent:

- `session_store = "memory"`: never persist anything. You authenticate once
  per process lifetime: one TUI session, or one daemon lifetime for the CLI.
- `reauth_sensitive = true`: actions that move money or items require a fresh
  login before they run (market buy/sell/cancel, and any future
  trade or wallet action). Login, presence and library reads don't.
  Those actions are already behind the off-by-default `market` and
  `inventory` features.

### Target model

| Setting (`config.toml`) | Default | Effect |
| --- | --- | --- |
| `session_store` | `keyring` | `keyring`: the refresh token and web token are separate Secret Service items. No file holds a secret. `memory`: nothing is persisted. |
| `reauth_sensitive` | `false` | A fresh login before reading the inventory, wallet or market listings (and any future market, wallet or trade action). |
| `daemon_idle_timeout` | `1800` (seconds) | The daemon drops its session and exits after this much idle time. |

If the keyring is unreachable, gaki **refuses to persist** and falls back to
`memory` with a warning. It never writes plaintext. Non-secret metadata
(account and persona name, used for "signed in as …") goes to
`$XDG_STATE_HOME/gaki/account.toml`.

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
- [x] **0.7** Make `LICENSE` the plain GPLv3 text. Drop the preamble: the
  MIT-until-commit and SteamFlow-derivation notes and the Aurelia copyright
  line. Git history already records all of it. Delete `gaki-tui/LICENSE`,
  because its GPLv3 text is the same. "Only" vs "or later" is set by each
  crate's `license` field, not by the text. **Caveat:** MIT requires its
  copyright and permission notice to ship with "substantial portions" of the
  code. While Aurelia's pre-GPL code is still in the tree, that notice moves
  to the README acknowledgements (7.2), next to the SteamFlow credit, instead
  of being deleted.
  *Done.* `LICENSE` now starts at the GPLv3 title (its first line re-indented to
  the canonical form), and `gaki-tui/LICENSE` is gone. The MIT notice, the Aurelia
  fork note and the SteamFlow derivation note sit in the README's License section
  for now, and the 7.2 split can move them. `gaki-tui/README.md` links the root
  `LICENSE`.

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
- [x] **2.2** Create `gaki-cli/` from `src/main.rs`, `cli.rs`, `commands/`, and
  `daemon/`. Change `aurelia::` paths to `libgaki::`. Rename the binary to
  `gaki-cli`.
  *Done.* `src/` moved to `gaki-cli/src/` (package and binary `gaki-cli`), the
  root `Cargo.toml` is now a pure workspace, and every `aurelia::` path is
  `libgaki::`. The root package is gone, so the integration tests moved here
  instead of in 2.5: `config_load_errors`, `login_warning` and
  `session_encryption` drive the binary and went to `gaki-cli/tests/`
  (`CARGO_BIN_EXE_gaki-cli`); the other 11 went to `libgaki/tests/`. The
  codecov matrix in `test.yml` now lists `libgaki` and `gaki-cli`. Two things
  had to follow the binary name now rather than in 2.4: clap's command name
  and the `kill`/`daemon list` process matcher in `proc_admin.rs`. Both use
  `env!("CARGO_BIN_NAME")`; otherwise `gaki-cli kill` would look for
  processes named `aurelia`. User-facing strings and env vars still say
  aurelia until 2.4. `deploy.yml` already lists `gaki-tui`, which arrives in
  Phase 3; a release tag before then would fail on that row.
- [x] **2.3** Move all logic out of `gaki-cli/src/commands/` into `libgaki`, so
  commands keep only argument mapping and output formatting (R23). Expect the
  heavy cases in `install.rs`, `info.rs`, `store.rs`, `auth.rs`, and
  `common.rs`. Output formatting (`core/output.rs`) moves to the CLI.
  The logic goes into a new `libgaki::ops` layer, one module per area, with
  typed arguments and `Serialize` results. It never prints, prompts or reads
  CLI flags, and it is the API `gaki-tui` will call in Phase 4. The CLI keeps
  argument mapping, prompts, text and JSON rendering, the daemon (D4), and
  its hints ("run `gaki-cli login`"). The `output.rs` move already happened
  in 2.1, and `capabilities` stays in the CLI because it describes the CLI
  itself. The work is split so each commit stays reviewable:
  - [x] **2.3a** Shared helpers from `common.rs`. Session restore, require-auth
    and the web-session fallback went to `ops::session`; the daemon branch
    and login hints stay in the CLI wrappers. `load_library`,
    `merge_family_shared`, `find_game` and `cached_game_name` went to
    `ops::library`. The active-install registry and `InstallGuard` went to
    `ops::install`. The stop/restart guard went to `ops::steam`, taking a
    callback for the CLI's "Stopping Steam" message. `RateTracker` and
    `percent_of` went to `ops::progress`, country resolution to `ops::store`,
    and `available_space_for` to `core::utils`. Added 11 tests.
  - [x] **2.3b** `runtimes.rs`, `config.rs` and `scripts.rs`. New
    `ops::config`:
    - `load`/`update` (load, edit, save) and `experimental`
      (`core::config::experimental_env_override` now owns the env-var check).
    - `update_proxy(ProxyChange)`, `games_with_settings` and
      `clear_all_game_settings`.
    - `update_game(app_id, GameSettingsChange)`. Its `Change<T>`
      (`Keep`/`Clear`/`Set`) replaces the CLI's flag-pair juggling.

    New `ops::proton`: `listing`, `find` (a typed `NotFound` that explains
    custom builds), `start_install`, `set_default` and `uninstall`. New
    `ops::scripts`: `list`, `create`, `show` and `remove`. The install-dir
    platform detection (`detect_installed_platform[_set]` and its bounded
    walk) moved from `commands/library.rs` to `libgaki::library::platform`,
    with tests, because `config game` needs it. `--json` output keeps its
    keys. Added 10 tests.
  - [x] **2.3c** `auth.rs`: login flows, logout, health. New `ops::auth`:
    - `login(client, &PasswordLogin)` makes one attempt and returns
      `LoginStep::{LoggedIn, CodeRequired(Email|Device), ApprovalRequired}`
      instead of the CLI inspecting `pending_confirmations`. Its timeout is
      now a typed `Timeout` error.
    - `login_qr`, and `secure_session` (run after every successful login).
    - `stored_session` (the encrypted-aware peek), `session_status` (shared
      with the daemon, whose `SessionStatus` is now this type),
      `verify_openid` and `store_web_token` (expiry and account-mismatch
      checks are typed `InvalidInput`).

    The CLI keeps prompts, the `AURELIA_PASSWORD`/`AURELIA_WEB_TOKEN`
    sources, the NDJSON driver events and cancel watching, QR rendering,
    the experimental gate message, and `--reconnect` (daemon-only). Added 5
    tests.
  - [x] **2.3d** `info.rs` and `store.rs`: metadata, cache, prices, search.
    *Done.* `ops::info` (cached batched `app_info`, DLC lists, branches,
    depots, achievements, launch options, DRM check, artwork) and
    `ops::store` (prices, search/deals/similar, wishlist, assets, news,
    reviews, tags). JSON shapes unchanged.
  - [x] **2.3e** `install.rs`: install/update/verify/uninstall, backlog,
    move/relink/import, DLC, downgrade.
    *Done.* `ops::install`: `prepare` → `PreparedInstall::run` (backlog
    bookkeeping), `signal`/`list`/`resume_request`/`forget`, verify, update,
    `check_updates`, move/relink/import, availability, manifests,
    `downgrade`, pin/unpin. Progress goes through a caller-supplied `drive`
    closure. Free space is checked before Steam is stopped.
  - [x] **2.3f** `library.rs`, `collections.rs`, `cloud.rs` and `launch.rs`.
    *Done.* `ops::library` (`list` with filters and collection memberships,
    `account`, `folders`), `ops::collections`, `ops::cloud` (`sync` →
    `SyncReport`, `files`), `ops::launch` (`targets`, `play` reporting
    `LaunchEvent`s, `running`, `stop`). Process admin (`kill`, `daemon
    stop/list`) stays in the CLI.
  - [x] **2.3g** `social.rs`, `market.rs` and `workshop.rs`. Afterwards,
    `commands/` holds only mapping and rendering; tick 2.3.
    *Done.* `ops::social` (friends, profiles, chat stream helpers, labels),
    `ops::market`, `ops::workshop` (browse with `Sort`, resolve, install,
    subscribe, `status` with update detection, ratings, comments). The last
    direct client calls in `commands/` (logout, appinfo debug) went through
    `ops` too.
- [x] **2.4** Drop the `AURELIA_*` environment variables and rename to gaki.
  Configuration comes only from CLI commands, flags and the config file.
  - [x] **2.4a** Replace every env var gaki *reads* with a flag or a config key,
    then delete the env lookup:

    | Env var | Replacement |
    | --- | --- |
    | `AURELIA_CONFIG_DIR` | global `--config-dir <PATH>` |
    | `AURELIA_NO_DAEMON` | global `--no-daemon` |
    | `AURELIA_NO_SPAWN` | global `--no-spawn` (use a running daemon, never start one) |
    | `AURELIA_DAEMON_SOCKET` | global `--socket <PATH>`; `daemon --socket` passes it in-process instead of via `set_var` |
    | `AURELIA_LOG`, `RUST_LOG` | global `--log <FILTER>` (alongside the existing verbosity flags) |
    | `AURELIA_EXPERIMENTAL` | `config experimental` only; drop `experimental_env_override` |
    | `AURELIA_INFO_CACHE_TTL` | config key `info_cache_ttl` (seconds; `0` disables) |
    | `AURELIA_SCRIPT_DIR` | config key `script_dir` |
    | `AURELIA_DISABLE_KEYRING` | config key `use_keyring` (default true) |
    | `AURELIA_PASSWORD` | `login --password-stdin` (or the prompt) |
    | `AURELIA_WEB_TOKEN` | `login --web-token <TOKEN>` / stdin, which already exist |

    Library code takes these as parameters or reads the config; only
    `gaki-cli` parses flags. Standard variables (`HOME`, `XDG_*`, proxy vars,
    `DISPLAY`, `COLUMNS`, `PATH`, `LD_*`) stay as they are. Update the docs,
    smoke-test recipe and CI that set these variables.
    *Done.* The CLI parses before anything reads the config, then calls
    `core::config::set_config_dir` and sets the daemon socket in-process. An
    auto-spawned daemon gets the client's `--config-dir`/`--socket`, and a
    forwarded command whose `--config-dir` differs from the daemon's is refused
    (exit 2). Sync paths read the new keys through
    `load_runtime_settings_blocking`. Daemon detection in `proc_admin` parses
    argv with the CLI definition, since global flags may precede `daemon`.
    Standalone stdin reads now share one buffered reader, so a Guard code
    piped after `--password-stdin` isn't lost. `config experimental --json` no
    longer has `env_override`. Smoke tests use
    `gaki-cli --config-dir <tmp> --no-daemon …` (a `--socket` path must be
    short: Unix sockets cap it near 108 bytes).
  - [x] **2.4b** Variables gaki *exports* to launch scripts and the game process
    (`AURELIA_APP_ID`, `_APP_NAME`, `_GAME_DIR`, `_LAUNCH_PROGRAM`,
    `_LAUNCH_ARGS`, `_STDOUT_LOG`, `_STDERR_LOG`) are the script interface and
    can't be flags: rename them to `GAKI_*`, including the generated script
    template.
    *Done.* Names live once as `launch_script::ENV_*` constants used by the
    stages, the tests and (by name) the template. The README's stale
    `AURELIA_CDN_CONCURRENCY` knob, which no code read, was dropped.
  - [x] **2.4c** Rename the rest: user-facing strings and command hints say gaki,
    the config dir moves to `gaki/`, the keyring service and user agent change.
    Grep for `aurelia` until only acknowledgements remain.
    *Done.* Command hints name `gaki-cli`, prose says gaki, and the config dir
    is `~/.config/gaki` (default library `~/Games/gaki`). The keyring service,
    user agent, session envelope tag (`gaki-session-v1`), daemon socket
    (`gaki-<uid>.sock`), launch lockfile, OpenID nonce, store query names and
    thread names all say gaki, and the process helpers are `GakiProcess` /
    `find_gaki_processes`. `aurelia` remains only in the forked crate names
    (`aurelia-steam-vent-chat`, `aurelia-steam-cdn`, Phase 6),
    `docs/aurelia-questions.md`, and the README's upstream install, crates,
    acknowledgement and link sections (Phase 7).
    **Known issue (fixed in 2.5):** some lib unit tests (runners, preflight,
    `steam_client`) write fixtures (`fake_env/`, `running/123.json`) into the
    real config dir instead of a temp one.
- [x] **2.5** Move integration tests to their owning crate's `tests/`. Create
  `examples/` (empty placeholder until Phase 5) and move documentation into
  `docs/` (leave the README content split for Phase 7). Point the codecov
  matrix in `test.yml` at the new crates.
  *Done.* The integration tests were already split in 2.1/2.2: the
  `libgaki/tests/` ones use only the lib, and the three that drive the binary
  sit in `gaki-cli/tests/`. `examples/` holds a README placeholder, and
  `aurelia-questions.md` moved to `docs/`. The codecov matrix already covered
  `libgaki` and `gaki-cli`; only its stale comment went. Also fixed the 2.4c
  known issue: under `cfg(test)`, `config_dir()` returns a per-process temp
  dir, so a full `cargo test` with a throwaway `HOME` writes nothing there.

### Phase 3 — import the TUI (R6, R7, R8, R9)

- [x] **3.1** Copy the `../Aurelia-TUI` sources into `gaki-tui/` as they are
  (record the upstream commit `d0af998` in the commit message). Name the crate
  `gaki-tui` and the binary `gaki`. It keeps talking to the CLI as a
  subprocess for now, but invokes `gaki-cli` instead of `aurelia`.
  *Done.* Copied everything but `.git`, `Cargo.lock` (the workspace has one)
  and the upstream `.gitignore` (it ignored `/docs` and `.c*`). The crate is a
  workspace member with the workspace version; its license stays the
  upstream `GPL-3.0-only` (now allowed in `deny.toml`), with the upstream
  `LICENSE` kept in `gaki-tui/`. `aurelia_tui::` paths became `gaki_tui::`.
  `bin()` looks for a `gaki-cli` next to its own executable, then on `PATH`;
  `AURELIA_BIN` stays as an override until Phase 4 drops the subprocess (a
  test relies on it). To pass the workspace gates: `cargo fmt`, clippy fixes
  (collapsed `if`s, a `Slot<T>` alias, an iterator loop, explicit
  `truncate(false)`), seven broken rustdoc links, one typo, `Dota` allowed in
  `.typos.toml`. CI installs `libssl-dev` (reqwest 0.11 uses native-tls until
  3.4) and codecov covers `gaki-tui`. Other aurelia names in the TUI (its
  strings, `~/.config/aurelia-tui`, `AURELIA_TUI_*`) are left for Phase 4/5.
- [x] **3.2** Strip Windows from the TUI: the `schannel` dependency, the
  `cmd`/`open` browser branches, and Windows path code.
  *Done.* Dropped the `schannel` dependency and its comments, the
  `cmd`/`open` branches (the browser is `xdg-open`), the `cmd /c start` chat
  window, `PlayOpts::windows` (`play --windows`, removed in 1.1), and the
  "Windows discovery" config row (its field is gone from gaki-cli). There was
  no Windows path code. `schannel` stays in `Cargo.lock` only as a
  Windows-target dependency of native-tls, until reqwest goes in 3.4.
  **Left for Phase 4:** the TUI still wraps commands removed in 1.4
  (`steam-runtime status|install|repair`, `config game … steam-runtime`,
  `play --steam-mode`) in the engine pane, game config and launch targets.
  They fail at runtime and go away when `interface/aurelia.rs` is ported.
- [x] **3.3** Strip HLTB: `interface/hltb.rs`, `ui/hltb.rs`,
  `tests/hltb_view.rs`, `tests/fixtures/hltb/`, `docs/HOWLONGTOBEAT.md`, and
  `scripts/test-hltb-terminal.py`. Also drop the TUI's `scripts/`, `assets/`
  screenshots, `FUNDING.yml`, and workflows. CI is unified at the workspace
  level.
  *Done.* Besides the listed files: `interface/hltb_tests.rs`, the
  `hltb_enabled` config switch (old configs that still carry it load fine),
  the detail-pane row, the settings row and `h` key, the `,ct` chord, the
  Actions entry, the help line, the HLTB state and tests in
  `interface/game.rs`, the README sections, and the `regex` dependency
  (HLTB-only). `assets/` keeps only `splash.txt`, which `app.rs` embeds; the
  logo, screenshots and the unused ASCII banner went with `scripts/`.
- [x] **3.4** Migrate `tui` 0.18 to `ratatui`, update crossterm, remove `atty`
  (use `std::io::IsTerminal`), and replace `tui-image-rgba-updated` with a
  maintained ratatui image widget, or drop images if none fits (D5).
  *Done.* `tui` 0.18 → `ratatui` 0.30 (`serde` feature). Crossterm comes
  re-exported as `ratatui::crossterm` (0.29), so the two can't drift.
  Mechanical changes: `Spans` → `Line`, `Table::default().rows(..)`,
  constraint/width arrays without `.as_ref()`, `Frame` without a backend
  parameter, `frame.area()`, and `Buffer::cell` / `Cell::symbol()` in tests.
  `atty` → `std::io::IsTerminal`. Images use `ratatui-image` 10 (ratatui org;
  `crossterm` feature only, no chafa or AVIF encoder) with the half-block
  protocol, built directly from the pre-sized art. Half-blocks keep the
  existing look, where the detail text is drawn over the faded background;
  kitty/sixel can't sit under text. `image` 0.24 → 0.25 (jpeg/png/webp).
  Beyond the plan: reqwest 0.11 + native-tls → the workspace's rustls reqwest
  0.13 with `blocking`, so OpenSSL leaves the tree and CI drops
  `libssl-dev`; `pretty-bytes` (unmaintained, pulled `atty`) → a local
  `util::bytes::convert` with the same output; `qrcode`, `serde` and
  `serde_json` use the workspace versions. `schannel` remains in
  `Cargo.lock` only as a Windows-target dependency of rustls-platform-verifier
  (as for libgaki).

### Phase 4 — TUI on `libgaki` (R23)

- [x] **4.1** Add the async bridge (D6): an `app/` worker that owns the tokio
  runtime and a `libgaki` session, plus typed request and response channels.
  *Done.* `gaki-tui/src/app/worker.rs`: `Worker::start()` builds a
  multi-thread runtime (16 MiB task stacks, like the CLI) and spawns the
  request loop. `request(Request) -> Ticket` and the non-blocking
  `poll() -> Option<Reply>` cross over channels; each request runs as its own
  task against a clone of one shared `SteamClient` (restores serialise on its
  lock); dropping the worker shuts the runtime down with a 1 s grace. The
  first requests are `SessionStatus` and `Restore`; 4.2 adds one per view.
  `app.rs` moved to `app/mod.rs` unchanged (its page builders go to `ui/` in
  4.5). `tests/worker.rs` round-trips both requests against an empty temp
  config dir, with no network. The worker isn't started from `main` yet: 4.2
  wires it in along with the proxy-env setup it needs.
- [x] **4.2** Port `interface/aurelia.rs` (subprocess calls plus output parsing)
  to direct `libgaki` calls, one view at a time: library, details, install and
  progress, launch and running, cloud, account and login, and the rest.
  *Done.* Every command runs in-process; nothing in `interface/aurelia.rs`
  spawns `gaki-cli` any more, and `run_json`/`json_error` are gone. Only the
  startup version check (`bin`, `version`, `AURELIA_BIN`) still runs the
  binary; 4.4 removes it. Pattern: each `interface/aurelia.rs` function keeps its
  signature and result type, but its body calls `libgaki::ops` through
  `in_process`, which runs the future on the shared worker
  (`app::worker::global()`, `Worker::block_on`) against its one session.
  Callers that were blocking on the subprocess block the same way for now;
  moving the UI-thread ones onto `Request`/`poll` follows once every view is
  in-process. `steam_urls` moved from `gaki-cli` to `libgaki::core`. `main`
  installs the proxy env before any thread starts.
  - [x] Session check, library list, library folders, store details, account
    (`health`, `fetch_library`, `libraries`, `fetch_info`, `account`).
    Also fixed a 2.4a regression: the standalone CLI calls passed the dropped
    `AURELIA_NO_DAEMON` env var instead of `--no-daemon`. Classic login
    still passes `AURELIA_PASSWORD`, which gaki-cli ignores since 2.4a; it
    goes with the login port.
  - [x] Install and progress (`install`, `install_estimate`, `install_stop`,
    new `install_pause`, `verify`, `update`, `uninstall`, `move_game`,
    `relink`, `import_game`). A shared `drive_progress` turns `libgaki`'s
    `DownloadProgress` into the same status text, with the CLI's
    `RateTracker` ETA. Pause now records a resumable backlog entry
    (`signal(id, pause = true)`) instead of a plain stop. Move, relink and
    import refuse while Steam runs, as the CLI did without `--restart-steam`.
  - [x] Launch and running (`play`, `play_with`, `launch_targets`,
    `running`, `stop`). The status cell follows `LaunchEvent`s (pre-launch
    update with progress, launching, the build picked). Games still outlive
    the TUI (no kill-on-drop or parent-death signal), and their output stays
    off the terminal (`child_stdio` is null at verbosity 0). Dropped
    `PlayOpts::native_engine`/`umu`: never set, and `play` lost those flags in
    Phase 1. `steam_mode` stays as a picker label. `libgaki`'s one stray
    `println!` (MangoHud missing, `wine_tkg.rs`) is now `tracing::warn!`, so
    it can't write over the TUI.
  - [x] Cloud (`cloud_list`, `cloud_sync`). Fixed silent partial syncs: the
    CLI's `--json` result carried conflicts, skipped and failed files with a
    success exit, so the TUI said "synced". Now any of them comes back as an
    error naming the count and the first file (worst first: failed,
    diverged, skipped). Diverged saves are still only detected; both copies
    are kept.
  - [x] Account and login (`login_qr`, `login_classic`, `logout`,
    `set_presence`, `config_show`, the proxy settings). Fixed username/password
    login, broken since 2.4a: the password went in an env var the CLI no
    longer reads. Logins now run on the worker and hand their client to the
    shared session, so later calls reuse it without restoring from disk.
    Logout resets the session. Presence changes are announced at once when
    logged in, instead of after a daemon restart. The QR flow now gets
    challenge URLs straight from `libgaki`; its rendering bug is 4.3.
  - [x] The rest
    - [x] Local settings: per-game config, Proton, launch scripts. Removed
      what Phase 1 had already dropped from the CLI, which the TUI still
      called and which failed at runtime: the runtime-plugins overlay
      (umu, luxtorpeda, Steam runtime; `ui/engine.rs`, the `E` key), the
      Runner, Steam runtime and Steam prefix rows of Game settings, the
      Prefix mode, Language, Shared compat data and Luxtorpeda settings
      rows, and the "Steam runtime (in Wine)" launch-target label. Proton
      matches 1.5: official builds install and uninstall through Steam and
      become the default; custom builds are listed but only the package
      manager removes them. The unused `scripts_dir`/`scripts_list` and
      `config_game_set_launch_script` wrappers are gone.
    - [x] Per-game Steam data: DLC, branches, depots, achievements, launch
      options, manifests, pinning, downgrade. Downgrades stream progress
      through the shared `drive_progress` (new `Downgrade` label); pin state
      is still read from local files without a Steam logon. That was the
      last streaming subprocess, so the NDJSON progress parser is gone too.
    - [x] Social and market: friends, chat, wallet, inventory, market,
      workshop. Fields are mapped from `libgaki` types directly instead of
      parsing the CLI's JSON. That fixes Workshop comments, which never
      loaded: the CLI gave the author as a numeric SteamID and the TUI
      expected a string. Subscribe and unsubscribe resolve the item first,
      so a collection covers its items, as in the CLI. Market search and
      price keep their background threads; the unused blocking variants are
      gone.
    - [x] Collections. The local store is edited directly (offline);
      pull, push and sync use the session. Negative app ids are rejected
      instead of being passed through.
- [x] **4.3** Fix the QR login bug (Q1). Render the QR challenge from the
  `libgaki` auth flow directly, rather than scraping it from CLI output, which
  is the most likely cause of the bug. Add a render test.
  *Done.* Root cause: in `--json` mode `gaki-cli login --qr` watches stdin
  for a `cancel` line and takes EOF as "driver gone". The TUI spawned it with
  stdin set to null, so the login aborted ("stdin closed; QR login
  abandoned"), usually before Steam sent a challenge. The 4.2 port removed
  the subprocess; the challenge now comes from `ops::auth::login_qr`
  directly. Also fixed: Esc didn't cancel, so an abandoned attempt kept
  running for up to 10 minutes, overwrote a newer attempt's code with its
  rotations, and threw you off the QR page when it timed out. Each attempt
  now has a number; a stale one stops at once and its code and outcome are
  dropped. The page shows only the code, centred, so it fits a standard
  80×24 terminal with its quiet zone; a smaller terminal gets the size it
  needs instead of a clipped code. `tests/qr_view.rs` decodes the drawn
  cells back into modules and compares them with the QR matrix.
- [x] **4.4** Delete the subprocess layer and drop `reqwest` blocking and
  `serde_json` CLI parsing from the TUI.
  *Done.* Removed `bin`, `AURELIA_BIN`, `version`, `version_supported`,
  `MIN_VERSION` and `STError::Process`; the footer badge shows the TUI's own
  version (`gaki vX.Y.Z`) instead of probing a binary. The interface structs
  lost their `Deserialize` derives, `#[serde]` attributes and lenient
  deserializers (`de_steam_id`, `de_opt_u64`, `default_true`), and
  `config_show` maps `LauncherConfig` directly. `serde_json` stays for the
  TUI's own settings file, the library cache and ProtonDB responses.
  Artwork and ProtonDB fetches go through `util::http`, one shared
  `libgaki::core::net::http_client` on the worker runtime, so `reqwest`
  lost its `blocking` feature. Docs and messages that still named the
  `aurelia` CLI now describe the in-process calls; sign-in hints say
  `gaki-cli login`. Still open: the TUI's own settings and cache live in
  `~/.config/aurelia-tui` and `~/.cache/aurelia-tui` (`util/paths.rs`).
- [x] **4.5** Bring `gaki-tui` into line with the ARCHITECTURE layout: `cli/`,
  `settings/`, `logger/`, `app/`, one file per element in `ui/`, and `utils/`.
  Split into four commits. Test files keep their current layout until 7.4.
  - [x] **4.5a** Entry plumbing. `util/` → `utils/`. A clap `cli/` replaces the
    hand-parsed `--chat <steamid> <name>` with `--chat <STEAMID> [--name <NAME>]`,
    and adds `--log-level`. `settings/` resolves `CliOptions` (plus the proxy
    config) into `Settings`. `logger/` sets up a `tracing` subscriber that writes
    to `$XDG_STATE_HOME/gaki/gaki-tui.log` and never to the terminal. That
    replaces the `log!` macro. `main` becomes parse → resolve → proxy → logger →
    run.
    *Done.* The log path comes from a new `libgaki::core::config::state_dir()`,
    which honours `XDG_STATE_HOME` and ignores a relative value, as the XDG spec
    says (D10's `account.toml` will use it too). The file is opened append-only
    and `0600`, since it can hold account names and SteamIDs. The default level
    is `info`, and `--log-level off` creates no file. A failure to set up logging
    prints a warning and the TUI starts anyway. The Friends panel's `t` now
    spawns `--chat <id> --name=<name>`, so a name starting with `-` isn't read as
    a flag. Errors from the run loop go to the log and to stderr, and the exit
    code is non-zero. **Found on the way:** `config_dir()` hard-codes
    `~/.config/gaki` and ignores `XDG_CONFIG_HOME`, which §11.5 requires; that
    fix fits 4.6, which moves the TUI's dirs anyway.
  - [x] **4.5b** Move the two run loops (`entry`, `chat_entry`) out of `main.rs`.
    Event handling and key dispatch go to `app/`, the frame drawing to `ui/`
    (`ui::draw`), and the artwork helpers (`cover_resize`, `fade_bottom`,
    `halfblock_art`) to `ui/artwork.rs`.
    *Done.* `main.rs` is down to 41 lines. `app/library.rs` holds the loop and
    the screen transitions (`advance`). `app/keys/` handles keys per screen
    (`login`, `browse`, `overlays`, `actions`), and `app/keys/shared.rs` holds
    install, pause/resume, cancel, favourite, hide and reload. Before, the
    Actions menu and the accelerators each had their own copy of those.
    `app/artwork.rs` and `app/marquee.rs` carry the art and marquee state that
    used to be loose locals. `app/chat.rs` runs the `--chat` window, and
    `app/terminal.rs` has a `TerminalGuard` that restores raw mode and the
    mouse on drop, so `main` no longer has to. On the drawing side,
    `ui::draw` dispatches to `ui/library.rs` (the layout), `ui/overlays.rs`
    (the overlay stack) and `ui/login.rs` (the old `App::build_*` builders,
    now free functions; the unused `build_login` is gone). The new
    `Browser::overlay_open` replaces a 30-flag condition.
    **Fixed on the way:** every quit exited with "The cursor position could
    not be read within a normal duration". `Terminal::clear` queries the
    cursor, and the input thread, blocked reading stdin, takes the reply. The
    clears after a login or reload ran the same risk mid-session.
    `app::terminal::clear` clears without asking (`clear_region(All)` plus
    `swap_buffers`). **Dropped:** the 17 `show_cursor`/`hide_cursor` calls.
    Nothing sets a cursor position, so each `draw` hid the cursor again and
    the calls did nothing. Checked in a scripted pty: sign-in page, username
    entry, Esc, quit with exit 0, and the `--chat` window. `HEAD` showed the
    quit error under the same script.
  - [x] **4.5c** Split `browse.rs` (the `Browser` state machine) into
    `app/browser/`, one file per view's state and actions.
    *Done.* `browse.rs` is gone. `app/browser/mod.rs` keeps the `Browser`
    struct, `new()` and the `Slot` type, and re-exports the public types. The
    methods sit in 23 files, one per view (`library`, `install`, `actions`,
    `relocate` for move/relink/import, `account` for account/settings/wallet,
    then one per overlay). The types moved with their view (`Filter`, `View`,
    `Sort`, `Counts` and `Badge` to `library`, `InstallPhase` to `install`,
    `Action` and `ActionRow` to `actions`), and `install_tests` became
    `install.rs`'s tests. Every `crate::browse::` path is now
    `crate::app::browser::`. Only `reset_selection` and `clamp_selection`
    became `pub(super)`; nothing else changed visibility. **Not done:** the
    fields still all live on `Browser` rather than in per-view sub-structs.
    That change would touch every `ui/` builder and integration test that
    reads `browser.show_*` and friends, so it fits better with 7.4 or with
    feature gating (6.x), where each view's state needs a `cfg` anyway. Some
    integration-test comments still describe `main.rs` layouts that no longer
    exist (`tests/browse_view.rs`, `tests/friends_chat.rs`); 7.4 covers them.
  - [x] **4.5d** Fold the rest: `client.rs` and `interface/` into `app/`,
    `config.rs` into `config/` (still JSON until Phase 5), and `theme.rs` into
    `ui/theme.rs`. `lib.rs` is left with `cli`, `config`, `settings`, `logger`,
    `app`, `ui` and `utils`.
    *Done.* `interface/aurelia.rs` is now `app/steam.rs`, since it's the TUI's
    view of Steam over `libgaki` and the `aurelia` name was left over from the
    CLI it once shelled out to; `aurelia::` call sites read `steam::`.
    `interface/game*.rs` became `app/game/` (`mod.rs` for `Game`, `status.rs`,
    and `protondb.rs`, which was `proton_data`). `client.rs` is
    `app/client.rs`, with its stale "shells out to the aurelia CLI" doc
    rewritten. `config.rs` is `config/` with its test in `config/tests.rs`,
    and `theme.rs` is `ui/theme.rs`. `tests/aurelia_integration.rs` became
    `tests/steam_integration.rs`. **Left as is:** the `*Json` struct names in
    `app/steam.rs` (`DlcJson`, `FriendJson`, …) date from parsing the CLI's
    JSON output. They're plain view models now, and renaming them touches
    every view, so that's a separate cleanup. The `aurelia-tui` directory
    names wait for 4.6.
- [x] **4.6** Rename the TUI binary from `gaki` to `gaki-tui` (D7): the `[[bin]]` name,
  the clap command name, the footer badge (`gaki-tui vX.Y.Z`), help text,
  `deploy.yml` artifacts, and docs. Move the TUI's own settings and cache from
  `~/.config/aurelia-tui` and `~/.cache/aurelia-tui` (`util/paths.rs`) to
  `gaki/tui/` under the gaki XDG dirs, so both binaries share one config root.
  *Done.* The binary, the clap name, the footer badge (`gaki-tui vX.Y.Z`), the
  error prefixes, `deploy.yml` and the ARCHITECTURE table all say `gaki-tui`.
  libgaki now has one `xdg_dir()` helper behind `config_dir()` (which now
  honours `XDG_CONFIG_HOME`, the gap found in 4.5a), a new `cache_dir()`
  (`XDG_CACHE_HOME`) and `state_dir()`. The TUI keeps its files in
  `gaki/tui/` under the config and cache dirs. The `AURELIA_TUI_DIR`,
  `AURELIA_TUI_CACHE_DIR` and `AURELIA_TUI_ICON_DIR` overrides are gone (per
  2.4a), and so is the `shellexpand` dependency that expanded them. On the
  first start, a `config.json` in the old `~/.config/aurelia-tui` is copied
  over, so favourites and hidden games survive. The old file is left in
  place, and a dir set with `set_config_dir` skips the copy. The old cache
  isn't migrated, since it refills itself. The integration tests isolate
  themselves through `set_config_dir` and `XDG_CACHE_HOME` instead of the old
  variables. Also replaced the splash's AURELIA wordmark and its "terminal
  client for the aurelia Steam CLI" tagline with GAKI and "a terminal Steam
  client". **Behaviour change:** anyone with a non-default `XDG_CONFIG_HOME`
  now gets `$XDG_CONFIG_HOME/gaki` rather than `~/.config/gaki`, so their
  session and config have to move or be redone once. `data_dir()` still
  returns the config dir; moving data to `XDG_DATA_HOME` belongs with
  Phase 5. Most of `gaki-tui/README.md` still describes the subprocess era;
  7.2 rewrites it.

- [x] **4.7** Clean up leftover names from earlier generations (Aurelia, Aurelia-TUI,
  steam-tui, OpenSteamClient) across the repo. *Done.* The 32 `*Json` view
  models in `app/steam.rs` are named for what they hold (`DlcJson` → `Dlc`,
  `ConfigJson` → `LauncherSettings`, `AvailableJson` → `PinState`, …), and
  their comments no longer describe CLI keys and string fallbacks that 4.4
  removed. `STError` (from steam-tui) is `TuiError`, and
  `opensteam_image_cache_dir` is `image_cache_dir`. Comments that still said
  "CLI call" for what is now in-process `libgaki` work are fixed, as are
  module names that no longer exist (`interface`, `util::http`). The root
  README lost Aurelia's install channels (AUR, Nix, crates.io, .deb,
  releases), its published-crates table, wiki links, star history, dead
  links to `CONTRIBUTING.md`, `SECURITY.md` and `CODE_OF_CONDUCT.md`, a broken
  `#why-gaki-cli` anchor, and false "related projects" claims. In their
  place: build from source, the three workspace crates, and links into
  `docs/`. The acknowledgments credit Aurelia, Aurelia-TUI and steam-tui
  too. `gaki-tui/README.md` now describes the in-process design, not the
  `aurelia` subprocess. `gaki-tui/docs/FEATURE_PLAN.md`, Aurelia-TUI's
  branch-workflow notes, is deleted. **Kept on purpose:** the
  `aurelia-steam-cdn`/`aurelia-steam-vent-chat` crate names (real
  dependencies), license provenance, the `aurelia-tui` legacy-config
  migration, and the credits. **Still stale, for 7.2:** the README's
  Windows/macOS claims (title, the Windows paragraph, `%USERPROFILE%`) and
  feature rows for code removed in Phase 1 (Windows Steam runtime, umu and
  luxtorpeda plugins, the GE/CachyOS download manager).

### Phase 5 — configuration overhaul (R26, R29, R30, R27)

- [x] **5.1** Add a `libgaki::config` module with TOML types for `config.toml`,
  `profiles/*.toml`, and `games.toml`, and loaders that discover XDG paths. Add
  `examples/` files and a test that parses every example, as lootrs does in
  `liblootrs/tests/examples.rs`.
  *Done.* `Catalog::load(config_dir, home)` reads all three files
  (`Catalog::load_user()` uses gaki's config dir). A missing file means
  defaults; an unreadable or unparsable one becomes a `LoadError` and the rest
  still loads. `Catalog::validate` returns lootrs-style `Issue`s: names,
  absolute paths after `~`, profile `name` matching the file stem, lootrs-only
  runners (`wine`, `umu`), duplicate app ids or aliases, all-digit aliases, and
  dangling profile references. `config.toml` and `games.toml` reject unknown
  keys; profiles ignore them, so lootrs' `dxvk`/`vkd3d`/`[wine]`/`[umu]` load.
  `config.toml` takes over `LauncherConfig`'s keys except `proton_version`
  (profiles replace it) and the keyring flags (5.9 replaces them). Pin state
  stays out of `games.toml`: it is written by gaki, not the user. A proton
  profile's `prefix` is optional; without one each game keeps its own Steam
  `compatdata/<app id>`. Nothing reads the new files yet (5.2, 5.3).
- [x] **5.2** Add a `libgaki::settings` resolver that merges the layers in the
  order CLI > game > profile > config > defaults and produces a resolved
  `LaunchSettings` for each game.
  *Done.* `settings::resolve(catalog, app_id, &Overrides, home)` is pure: no
  I/O, `~` expanded. The profile comes from the CLI, then the game, then
  `default_profile`. The platform comes from the CLI (`--proton` implies
  Windows), then the game, then an explicitly chosen profile's runner, then
  `default_platform`; unset, the launch picks from the game's builds. An
  explicit profile that contradicts the platform is an error, while the
  default profile is just left out. Proton-only values (build, prefix, DLL
  overrides, anti-cheat) sit in `LaunchSettings::proton`. Env merges profile
  then game. The `scripts/<app id>.sh` fallback is left to 5.5, and the
  launch path doesn't call the resolver yet (5.3).
- [x] **5.3** Rework the launch pipeline (`launch/stages/resolve_profile.rs`,
  `prepare_prefix.rs`, and `build_environment.rs`) to use the resolved profile:
  the shared prefix becomes `STEAM_COMPAT_DATA_PATH`, and the pipeline adds
  wrappers, env, and DLL overrides. Steam-style per-game `compatdata/<appid>`
  is no longer created.
  *Done.* `ops::launch::play` resolves `LaunchSettings` once (the CLI's
  `--proton`/`--script`/`--no-script`/`--platform` as `Overrides`) and passes
  them down; `ResolveProfileStage` resolves them for callers that didn't. A
  profile's `prefix` becomes `STEAM_COMPAT_DATA_PATH` (`WINEPREFIX` is its
  `pfx`) for the launch, the running-game record, cloud sync, and DLL
  detection; `compatdata/<appid>` is only used, and created, for a game whose
  profile has no prefix (5.1). The Proton runner merges the profile's
  `dll_overrides` over the computed ones, puts `runner_args` before the game,
  and sets `PROTON_LOG` for `debug` and `PROTON_EAC_RUNTIME` /
  `PROTON_BATTLEYE_RUNTIME` for `[anticheat]` when those runtimes are
  installed. The empty `BuildEnvironmentStage` is gone; a new
  `ApplySettingsStage` (after preflight, before the launch script) adds env,
  the game's `args`, and the wrappers for both runners. The profile's Proton
  build is used when the launch runs the Windows build; choosing *which* build
  from `settings.platform` is 5.4. A game's `script` from `games.toml` now
  applies too; the alias and old-path fallback remain for 5.5.
- [x] **5.4** Add `default_platform` (R27/Q5). `--platform` overrides it, and
  `games.toml` can set it per game.
  *Done.* `play`, `launch-targets` and `install` all take the build from the
  resolved `settings.platform`: `--platform` (or `--proton`, which means
  Windows), then the `games.toml` entry, then an explicitly chosen profile,
  then `default_platform`. The game's old `config.json` platform and forced
  Proton still sit between the command line and `games.toml` until 5.7 moves
  the TUI's game settings over. `install` only uses the configured platform
  when the game ships it, otherwise it auto-selects as before (Windows first);
  with no platform anywhere, a launch still runs the installed build. With a
  Windows preference the profile's Proton build is used ahead of the global
  `proton_version`.
- [x] **5.5** Add a named `script` path per game, with a fallback to
  `scripts/<appid>.sh` (R30/Q8), and game `name` aliases accepted wherever the
  CLI takes an app id.
  *Done.* `launch_script::resolve` order: `--no-script`, `--script`, the game's
  `script` in `games.toml`, the old `config.json` `launch_script`, then
  `<script_dir>/<app id>.sh` if it exists; `script_dir` comes from
  `config.toml` before `config.json`. `scripts show` and the launch agree.
  Every CLI app-id argument (52 of them) goes through one `app_ref` value
  parser, which takes an app id or a `games.toml` `name`
  (`libgaki::config::resolve_app_ref`); plain ids read no file. `main` finds
  `--config-dir` before clap runs so aliases resolve against the right
  directory. The `scripts` and `config script-dir` commands still write
  `config.json` until 5.7.
- [x] **5.6** Add a per-profile/per-game Steam integration mode (`host`,
  `none`, `auto`), replacing the dropped `--steam-mode`. `auto` keeps today's
  behaviour: bridge to host Steam when it is running or `--steam` is given.
  *Done.* `steam = "auto" | "host" | "none"` in a profile or a `games.toml`
  entry, resolved `--steam` (host) > game > profile > `auto` into
  `LaunchSettings::steam`. `host` does what `--steam` did (start Steam, wait
  for logon, always bridge, skip the DRM preflight); `none` keeps the Proton
  launch standalone even with Steam running; a Family-Shared game still
  forces `host`, with a warning when `none` was asked for. `launch-targets`
  marks its default the same way. Settings are now resolved before the
  pre-launch update, so a config error fails before anything downloads.
- [x] **5.7** CLI: rewrite the `config` subcommand for the new model and add a
  `profiles list|show` command. TUI: wire `ui/game_config.rs` to profile
  selection and per-game overrides. Split into:
  - [x] **5.7a** TOML writes and the `config` command. *Done.*
    `libgaki::config::edit` edits `config.toml`/`games.toml` in place with
    `toml_edit`, keeping comments; every save is parsed and validated against
    the whole config dir first (an unknown `default_profile` or game
    `profile` is refused) and written atomically. Until 5.8, the
    `config.toml` keys present in the file are laid over the `config.json`
    values they replace in `load_launcher_config` and the blocking readers, so
    every reader sees them. `gaki-cli config`: `show` prints `config.toml`
    with defaults, new `check`, `get`, `set`, `unset` cover every key, and
    `presence`, `experimental`, `install-resume`, `proxy`, `script-dir`,
    `info-cache-ttl` now write `config.toml` (the TUI's presence and proxy
    too). `config game` edits the `games.toml` entry (`--profile`,
    `--platform`, `--script`, `--steam`, `--name`, `--clear`) and drops that
    game's old `config.json` launch settings, which would otherwise still
    win; its `--proton` is gone (a per-game Proton is a profile). `keyring`
    stays in `config.json` until 5.9. Known wrinkle until 5.8: a JSON save
    (pins, keyring) writes the overlaid TOML values into `config.json` too.
  - [x] **5.7b** `gaki-cli profiles list|show`. *Done.* `list` shows each
    profile's runner and Proton build, how many games name it, its prefix,
    the `default_profile` marker, profiles with errors, and profile files
    that didn't load; `show` prints the file itself, the games using it
    (with aliases), and its issues. Both take `--json` and run locally, not
    through the daemon (they only read files), and follow `profiles_dir`.
    Profiles with a lootrs-only runner now report just that error, not
    warnings about fields that mean something else in lootrs.
  - [x] **5.7c** Drop launch scripts for per-game `wrappers`. A script was
    already run exactly like a wrapper (the command as `"$@"`), so wrappers
    cover it. *Done.* `games.toml` entries take `wrappers`, applied inside
    the profile's (a leading `~/` is expanded); every launch exports the
    `GAKI_*` variables (`launch::game_env`) before wrapping. Removed: the
    `script` key, `script_dir`, the `scripts/<app id>.sh` fallback,
    `launch_script`, `ApplyLaunchScriptStage`, `ops::scripts`, the
    `scripts` command, `play --script/--no-script`, `config script-dir`,
    `config game --script` (now `--wrapper`/`--no-wrappers`), and the
    TUI's launch-script row. `config check` warns about each leftover
    `<config dir>/scripts/*.sh` and how to keep it as a wrapper. An old
    `script` key in `games.toml` fails to load with "unknown field".
  - [x] **5.7d** TUI: `game_config` on `games.toml` with profile selection;
    drop the JSON `update_game`. *Done.* The Game settings overlay has
    Profile, Platform and Steam rows, all written to the `games.toml` entry
    through `update_game_entry` (so the game's old `config.json` forced
    Proton and platform are dropped on the first change). An unset row
    shows what it falls back to (`default_profile`, the profile's Steam
    integration). The wrappers are shown read-only, with the CLI command
    to change them. Removed: the TUI's forced-Proton row,
    `GameSettingsChange`, `GameSettings`, `update_game`, `platform_name`
    and `Change::apply_to` from `ops::config`.
- [x] **5.8** Remove the JSON config code paths. *Decided:* every file gaki
  stores becomes TOML, in the same directory under the same name, caches
  included (`--json` output and Steam's JSON APIs are not files and stay).
  No migration: `config check` warns about a leftover `config.json` or
  `user_apps.json`. Split into:
  - [x] **5.8a** Settings. *Done.* `config.json` is gone: its last settings
    are `config.toml` keys, `default_proton` (what `proton default` sets; the
    build for a Windows launch whose profile names none) and, until 5.9,
    `keyring` and `encrypt_session`. `LauncherConfig` is now a plain view of
    `config.toml` (`from_config`), so the `TomlOverlay` bridge, the blocking
    JSON readers, `ops::config::update` and `save_launcher_config` are gone.
    Version pins live in `<config dir>/pins.toml` (one table per app id,
    depot = "manifest id", strings since manifest ids exceed TOML's integer
    range), written atomically (`config::edit::write_atomic`). The old
    per-game forced Proton and platform (`GameConfig`, `BuildPreference`)
    and `user_apps.json` (launch options, env, graphics layers; nothing wrote
    it) no longer reach the launch; the in-memory `UserAppConfig` plumbing
    in the runner stays for now. The TUI's settings moved to
    `tui/config.toml`; a broken file is logged and left alone instead of
    being overwritten with defaults, and the copy-over of an
    `~/.config/aurelia-tui/config.json` is gone.
  - [x] **5.8b** gaki's state files to TOML, same paths:
    `install_backlog`, `collections`, `running/<app id>`, the cloud-sync
    state. (`session.json` goes in 5.9.) *Done.* All four go through
    `core::store::{read, write}` (TOML, atomic temp-file-and-rename; a
    missing file is `None`); collections and the cloud-sync baselines are
    now written atomically too. A dynamic collection's `filter_spec` is
    Steam's opaque JSON, which can hold `null`, so it is kept as JSON text
    in a TOML string (`store::json_text`). `config check` also warns about
    a leftover `collections.json` (unpushed edits are only there) and
    `install_backlog.json`; old `running/*.json` and `cloud_sync/*.json`
    are ignored (a missing baseline only means the next sync compares
    from scratch).
  - [x] **5.8c** Caches and launch-log artifacts to TOML: `library_cache`,
    the info and name caches, the TUI's `games` cache, `summary`,
    `effective_env` and the other log files. Drop `serde_json` where only
    files used it. *Done.* All go through `core::store` (the TUI writes its
    cache with `write_atomic`). TOML can't hold a bare list or a missing
    value inside an array, so the lists sit under a key (`[[games]]`,
    `[[resolution]]`) and the info cache's DLC `(id, name)` pairs are
    `[[dlc]]` tables. The launch event log is `events.toml`, one
    `[[event]]` table appended per event, so the file stays valid TOML
    (`EventLog::parse` reads it back). Old `.json` caches are ignored and
    refill themselves. `serde_json` stays in all three crates: Steam's web
    APIs, ProtonDB, `--json` output and `session.json` (until 5.9) use it.
    The only JSON file gaki still writes is `session.json`.

**Credentials (D10).** The following steps implement the model described in
"Authentication and session security". They come after 5.1, which adds the
`config.toml` keys.

- [x] **5.9** Add a `libgaki::auth::store` module with a `SecretStore` trait
  and two backends, `Keyring` (one Secret Service item per secret:
  `gaki`/`refresh-token:<steamid>` and `gaki`/`web-token:<steamid>`) and
  `Memory`. Add `session_store`. Delete `session.json`, the ChaCha/Argon2
  envelope (`core/session_crypto.rs`), the legacy password mode,
  `encrypt_session`, `use_keyring`, and the `argon2`, `chacha20poly1305` and
  `hex` dependencies if nothing else uses them. Never fall back to plaintext.
  Non-secret account metadata goes to `$XDG_STATE_HOME/gaki/account.toml`.
  A user with an old `session.json` logs in once; gaki then deletes the file.
  *Done.* `libgaki::auth`: `store` (the trait, `Keyring`, `Memory`,
  `session_store`, `set_session_store`) and `session` (`load_session`,
  `save_session`, `delete_session`, `stored_account`). `account.toml` holds
  the names, SteamID and which tokens the store has, so a token the keyring
  lost is a "log in again" error naming the store instead of a silent
  logout; it sits in the config dir instead when one is set for the process
  (`--config-dir`, tests). A save also removes the tokens of an account it
  replaces, an old `session.json`, and the old `gaki`/`session-key` item.
  An old `session.json` without `account.toml` asks for a login (`config
  check` warns about it too). Without a keyring the error says so and
  points at `secret_store = "memory"`, a new `config.toml` key (settings
  are never environment variables) that keeps tokens in the running daemon
  only. The daemon now watches `account.toml`. `logout` no longer restores
  first, so a session that can't be restored can still be removed. Removed:
  `config keyring`, the `keyring`/`encrypt_session` keys, `argon2`,
  `chacha20poly1305`; `hex` stays (the depot browser uses it).
- [x] **5.10** Token hygiene. Wrap tokens in `secrecy::SecretString`:
  redacted `Debug`, zeroized on drop, and no `Serialize`. Add a test that
  `{:?}` of every session type contains no token. Grep `tracing` calls for
  token fields.
  *Done.* `SessionState`'s `refresh_token`/`web_token`, `WebTokenInfo::token`,
  `WebSession`'s cookie header (it carries `steamLoginSecure`) and, beyond
  the plan, `PasswordLogin::password` are `SecretString`s; `SessionState`
  lost `Serialize`/`Deserialize` (`account.toml` has its own type), and
  `WebSession` got a `Debug` that shows only the SteamID and the redacted
  cookie. `auth::redaction_tests` checks `{:?}` and `{:#?}` of all four.
  The `tracing`/print grep found no token values logged (the CDN auth token
  is logged by length only), and no token is put in a URL, so request
  errors can't echo one. Left as is: the plain copies handed to
  steam-vent's `Connection::access`/login and the TUI's password input
  buffer.
- [x] **5.11** Make logout revoke at Steam. Call
  `Authentication.RevokeRefreshToken` before deleting the local copy. If
  steam-vent doesn't expose it, hand-wire it as the CloudConfigStore RPC is.
  If revocation fails, delete locally anyway and say the token may still be
  valid until it expires.
  *Done.* steam-vent already exposes `Authentication.RevokeToken#1`
  (`CAuthentication_Token_Revoke_Request`: the token plus a revoke action),
  which is how a client revokes its own refresh token, so nothing needed
  hand-wiring; `RevokeRefreshToken` takes a token id and a device signature
  and is for revoking other sessions. `SteamClient::revoke_refresh_token`
  sends it as `k_EAuthTokenRevokePermanent`. `ops::auth::logout` restores
  the session if needed (the daemon and the TUI use their live one), revokes
  within 20 s, then deletes the local session whatever happened, and returns
  a `Revocation` (`revoked`, `no_token`, `failed` with the reason). The CLI
  prints it (`--json`: `"token": {"revocation": ...}`); on failure it warns
  that a copy may work until it expires. The TUI's logout no longer needs a
  restorable session either. A browser web token can't be revoked; it
  expires on its own. Not tested against Steam (needs an account).
- [x] **5.12** Harden the daemon. The socket lives only under
  `$XDG_RUNTIME_DIR` (with no `/tmp` fallback; refuse to start without it).
  Create it `0600` inside a `0700` `gaki/` directory, check `SO_PEERCRED` so
  only the same uid can connect, and enforce `daemon_idle_timeout`. The daemon
  reads the token from the store itself; it never travels over the socket.
  *Done.* The socket is `$XDG_RUNTIME_DIR/gaki/daemon.sock`; an unset, empty
  or relative `XDG_RUNTIME_DIR` means no daemon (`gaki-cli daemon` fails, a
  client runs the command locally without trying to spawn one). `gaki/` is
  created `0700`; an existing one we own is tightened, anything else is
  refused. The socket is chmod `0600` after bind, `--socket` paths included,
  and `accept` drops a peer whose `SO_PEERCRED` uid isn't ours, with a
  warning. `daemon_idle_timeout` is a `config.toml` key in seconds (default
  1800, `0` never, like `info_cache_ttl`), re-read every 30 s check; the
  daemon exits when no request, install or game is running and it has been
  quiet that long, removing its marker and socket. The version marker is now
  `daemon.toml` next to the socket (it was JSON at `gaki-<uid>.info`). The
  socket protocol carries argv, stdio and an exit code, never a token.
- [x] **5.13** Add `reauth_sensitive`. A `Sensitive` marker on the
  market/wallet/trade `ops` makes them demand a fresh `LoginStep` flow
  before they run. The CLI prompts, and the TUI opens the login page and
  resumes the action afterwards. Document the threat model in `docs/FAQ.md`
  (7.2).
  *Done.* gaki has no buy, sell, cancel or trade action, so (your call) the
  gate covers the account reads: `ops::market::{inventory, wallet,
  listings}` take a `SensitiveGrant` by value, and only
  `ops::auth::sensitive_grant()` makes one. With the `reauth_sensitive`
  key off it is free; with it on it needs a password or QR login
  (`ops::auth::login`/`login_qr`) finished in this process within
  `REAUTH_WINDOW` (5 min), and each login is good for one grant. A
  restored session never counts. Prices and search stay open. The CLI runs
  an interactive sensitive command locally (it needs the terminal, like
  `login`), signs in as the stored account (empty password means a QR
  code) and uses that fresh client for the web session; `--json` fails
  with `auth_required` unless the process has an unused fresh login (the
  daemon after `login --json`). The TUI's wallet, inventory and listings
  go through `App::open_sensitive`: the sign-in page names the view, `Esc`
  goes back, and the view opens once the library is back. Open: each
  fresh login, like every `login`, mints a new refresh token and leaves the
  previous one valid at Steam until it expires (not revoked). Any future
  buy/sell/trade op must take a `SensitiveGrant`. The FAQ write-up stays
  with 7.2.

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
  **HLTB (plans2):** add it only if it can search by Steam app id, and it
  can't. HowLongToBeat has no public URL that looks up a game by Steam app id.
  Its own search takes a title, and the community tools that map app ids
  scrape HLTB pages. A title search (`howlongtobeat.com/?q=<name>`) would be
  name-based and can match the wrong game, so it stays out unless you want it
  anyway.
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
  **`aws-lc-sys` stays (plans2).** Its license is
  `ISC AND (Apache-2.0 OR ISC) AND OpenSSL`. You've decided this is acceptable
  for GPLv3. Keep the `OpenSSL` exception in `deny.toml` and reword its comment
  so it records the decision instead of pointing here as a to-do. (For the
  record: the FSF considers the old OpenSSL license GPL-incompatible. Switching
  rustls to `ring` remains a one-feature change if that ever matters, for
  example for a distro package.) Once `cargo deny check` passes in full, switch
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

## Q&A carried over from `docs/aurelia-questions.md`

| Q | Resolution |
| - | ---------- |
| Q1 QR in TUI broken | Step 4.3 |
| Q2 AUR `!lto` | Step 0.4. Re-verify when packaging. |
| Q3 CI/CD | Step 0.5 |
| Q4 Steam required? | Yes, by design (R21). Documented in step 7.2. |
| Q5 default platform | Step 5.4 |
| Q6 slim builds | Phase 6 |
| Q7 umu download / PATH | Moot. umu support is removed (step 1.3). |
| Q8 named scripts | Steps 5.5, 5.7c (scripts became per-game `wrappers`) |
| Q9 ProtonDB/GitHub hotkey | Step 7.1 |
| Profiles / shared prefixes | Phase 5 |
| plans2: reuse liblootrs profiles | D9: shared file format, no dependency (5.1) |
| plans2: rename binary to `gaki-tui` | Step 4.6 |
| plans2: pure GPLv3 `LICENSE` | Step 0.7 |
| plans2: HLTB link by app id | Not possible; stays a non-goal (7.1) |
| plans2: `aws-lc-sys` with GPLv3 | Kept (7.3) |
| plans2: auth and session safety | D10, steps 5.9–5.13 |

`docs/aurelia-questions.md` can be deleted once this plan is accepted.
