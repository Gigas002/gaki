# Configuration

gaki is configured with TOML files, never environment variables. Every file is
optional, and [`examples/`](../examples) has a commented copy of each. Change
them by hand or with `gaki-cli config` commands, which keep your comments, and
check them with `gaki-cli config check`.

## Where things live

| Directory                                      | Contents                                                       |
| ---------------------------------------------- | -------------------------------------------------------------- |
| `$XDG_CONFIG_HOME/gaki/` (`~/.config/gaki/`)   | `config.toml`, `games.toml`, `profiles/`, plus what gaki writes there: `pins.toml`, `install_backlog.toml`, `library_cache.toml`, `info_cache/`, `images/`, launch `logs/` |
| `$XDG_STATE_HOME/gaki/` (`~/.local/state/gaki/`) | `account.toml` (the session, without its tokens), `gaki-tui.log` |
| `$XDG_CONFIG_HOME/gaki/tui/`                   | `gaki-tui`'s own `config.toml` ([GAKI-TUI.md](GAKI-TUI.md))     |
| `$XDG_CACHE_HOME/gaki/tui/`                    | `gaki-tui`'s cached library and cover art                       |
| `$XDG_RUNTIME_DIR/gaki/`                       | the session daemon's socket                                    |

`--config-dir <PATH>` moves the config directory, `account.toml` included. An
embedding driver (Heroic, for example) can use it to keep gaki's state apart
from a standalone install. One daemon serves one config directory, so pair it
with `--socket <PATH>` or `--no-daemon`.

Paths in any file may start with `~`.

## Precedence

Settings apply in this order, later ones winning:

1. built-in defaults
2. `config.toml`
3. the game's profile
4. the game's `games.toml` entry
5. command-line flags

## `config.toml`

Global settings. `gaki-cli config set <key> <value>` checks a value before it
writes it, and `config unset <key>` removes one.

| Key                   | Default              | Meaning                                                    |
| --------------------- | -------------------- | ---------------------------------------------------------- |
| `steam_library`       | the detected library | where games install                                        |
| `profiles_dir`        | `<config>/profiles`  | where profiles live; point it at `~/.config/lootrs/profiles` to share lootrs' |
| `default_profile`     | —                    | the profile a game uses when `games.toml` names none       |
| `default_platform`    | —                    | `windows` or `linux`: the build to install and run when a game has both. Unset installs the Windows build and runs whichever is installed |
| `default_proton`      | `experimental`       | the Proton build for a Windows launch whose profile names none: an official build name or a path. `gaki-cli proton default` sets it |
| `cloud_sync`          | `true`               | sync Steam Cloud saves around a launch                     |
| `presence`            | `offline`            | your Steam presence while gaki is signed in: `offline` or `online` |
| `proxy`               | —                    | a proxy for all HTTP(S) traffic, e.g. `socks5://127.0.0.1:1080`. Steam's rules forbid using one to disguise your country ([STEAM-RULES.md](STEAM-RULES.md#your-part)) |
| `no_proxy`            | —                    | comma-separated hosts that bypass it                       |
| `install_resume`      | `manual`             | `auto` resumes interrupted installs when the daemon starts |
| `experimental`        | `false`              | turn on experimental features                              |
| `info_cache_ttl`      | 6 hours              | seconds a cached `info` record lives; `0` turns the cache off |
| `daemon_idle_timeout` | 30 minutes           | seconds the daemon idles before it exits; `0` never        |
| `reauth_sensitive`    | `false`              | ask for a fresh sign-in before showing the inventory, wallet or market listings |
| `secret_store`        | `keyring`            | where tokens are kept: `keyring` or `memory`               |
| `shader_cache`        | `<cache>/shadercache` | where games' compiled shaders go: a directory that gets one `<app id>/` per game, or `off` (see [Shader caches](#shader-caches)) |

## Profiles

A profile describes one runner, one prefix and their environment, and any
number of games can share it. That's how gaki avoids Steam's one-prefix-per-game
sprawl. Each profile is `profiles/<name>.toml`, and its `name` must match the
file name.

```toml
name = "proton"
runner = "proton"                 # proton | native
prefix = "~/prefixes/proton"      # shared by every game using this profile
wrappers = ["gamemoderun"]
steam = "auto"                    # auto | host | none

[proton]
build = "experimental"            # an official build name, or a path

[env]
PROTON_ENABLE_WAYLAND = "1"

[dll_overrides]
winhttp = "n,b"
```

| Key             | Meaning                                                                   |
| --------------- | ------------------------------------------------------------------------- |
| `name`          | the profile's name, the same as its file name                             |
| `runner`        | `proton` or `native`. `wine` and `umu` parse, so lootrs profiles load, but gaki can't launch them and `config check` says so |
| `prefix`        | for `proton`, the `STEAM_COMPAT_DATA_PATH` (its `pfx/` is the Wine prefix). Leave it out to give each game its own Steam `compatdata/<app id>`. For lootrs' `wine` and `umu` it is the Wine prefix itself |
| `runner_args`   | extra arguments for the runner, before the game's own                     |
| `wrappers`      | commands the launch is wrapped in, outermost first                        |
| `debug`         | turn on the runner's own logging                                          |
| `env`           | environment variables for the game                                        |
| `dll_overrides` | DLL name to Wine override mode (`n`, `b`, `n,b`, or `""` to disable)      |
| `proton.build`  | an official build name (`experimental`, `9.0`, ...) or the path of a Proton directory, such as a GE-Proton or proton-cachyos build from your package manager |
| `steam`         | the Steam integration for games using the profile (see below)             |
| `anticheat`     | `battleye` and `eac` (booleans): anti-cheat runtimes to make available    |
| `shader_cache`  | `off` or a directory, over `config.toml`'s (see [Shader caches](#shader-caches)) |

The format is lootrs': gaki reads profiles through lootrs' own model, so one
profile directory can serve both tools. lootrs' fields (`dxvk`, `vkd3d`,
`[wine]`, `[umu]`) load and gaki ignores them. lootrs in turn loads `proton` profiles, `[proton]` and `steam`, but
can't launch a `proton` profile.
`gaki-cli profiles list` and `gaki-cli profiles show <name>` show what gaki
loaded, which games use each profile, and any problems.

## `games.toml`

A game needs an entry only when it overrides something. `gaki-cli config game
<app id>` edits one without opening the file.

```toml
[[game]]
app_id = 292030
name = "witcher3"                 # an alias, usable anywhere an app id is
profile = "proton-cachyos"
platform = "windows"
args = ["-skipIntro"]
wrappers = ["~/games/run_scripts/witcher3/script.sh"]
steam = "host"
[game.env]
DXVK_HUD = "fps"
```

| Key        | Meaning                                                             |
| ---------- | ------------------------------------------------------------------- |
| `app_id`   | the game                                                            |
| `name`     | an alias for the app id                                             |
| `profile`  | the profile to run it with                                          |
| `platform` | `windows` or `linux`: the build to install and run                  |
| `args`     | extra arguments for the game                                        |
| `wrappers` | commands wrapped around the launch, inside the profile's            |
| `env`      | environment variables, over the profile's                           |
| `steam`    | the Steam integration, over the profile's                           |
| `shader_cache` | `off` or a directory, over the profile's                        |

### Wrappers

`wrappers` wrap the fully resolved launch command, the profile's outside the
game's: `mangohud`, `gamemoderun`, `gamescope -f --`, or your own script. Each
gets the rest of the command as its arguments, so a script ends with
`exec "$@"`. gaki also exports `GAKI_APP_ID`, `GAKI_APP_NAME`, `GAKI_GAME_DIR`,
`GAKI_LAUNCH_PROGRAM` and `GAKI_LAUNCH_ARGS`.

### Steam integration

`steam` (in a profile or a game entry) decides whether a launch bridges to the
host Steam client, which Steamworks features and some DRM need:

| Value  | Behavior                                                 |
| ------ | -------------------------------------------------------- |
| `auto` | bridge when Steam is running (the default)               |
| `host` | always bridge, starting Steam when needed (`play --steam`) |
| `none` | never bridge                                             |

Family-Shared games always bridge: they need the host Steam client.

### Shader caches

DXVK, VKD3D-Proton and the GPU drivers keep the shaders they compile on disk
so a game doesn't stutter compiling them again. gaki gives each game its own
cache directory, `$XDG_CACHE_HOME/gaki/shadercache/<app id>` by default, and
points them at it for Proton and native launches alike:

| Variable                      | In the game's directory |
| ----------------------------- | ----------------------- |
| `DXVK_STATE_CACHE_PATH`       | `DXVK_state_cache/`     |
| `VKD3D_SHADER_CACHE_PATH`     | `VKD3D_shader_cache/`   |
| `MESA_SHADER_CACHE_DIR`       | the directory itself    |
| `__GL_SHADER_DISK_CACHE_PATH` | `nvidiav1/` (NVIDIA)    |
| `AMD_VK_PIPELINE_CACHE_PATH`  | `AMDv1/` (AMDVLK)       |
| `STEAM_COMPAT_SHADER_PATH`    | the directory itself (Proton only) |

This is the layout Proton uses for its local shader cache. `shader_cache` in
`config.toml`, a profile or a game entry moves it (`<path>/<app id>`), or
turns it `off`, which sets none of these and leaves each one to its own
default. A variable that is already set wins: in the profile's or game's
`env`, by a fixup, or in gaki's own environment. `gaki-cli uninstall`
deletes the game's cache, `gaki-cli available` shows its size, and
`config check` reports a `shader_cache` that can't be created. Caches left
at the old defaults (a `.dxvk-cache` next to the game, `~/.cache/mesa_shader_cache`,
which other programs share) are not moved or deleted.

## Session and security

- **Tokens.** The Steam refresh token, and a browser web token if you added
  one, are kept in the OS keyring (Secret Service), one item per token. gaki
  never writes a token to a file. The rest of the session (the account and
  persona name, the SteamID) is in `account.toml`.
- **No keyring.** `config set secret_store memory` keeps the tokens in the
  running daemon only, so a daemon restart or an idle exit needs a new login.
- **Logout** revokes the token at Steam before deleting the local copy.
- **`reauth_sensitive`** makes the inventory, wallet and market listings ask
  for your password (or a QR code) each time, in `gaki-cli` and `gaki-tui`
  alike, so a copied session can't read them. A sign-in counts for one view,
  within five minutes. Prices and market search are unaffected. `--json`
  can't prompt, so those commands fail there with `auth_required`.
- A `session.json` from an older gaki isn't read. Log in once and it's
  deleted.

### The session daemon

The daemon's socket is `$XDG_RUNTIME_DIR/gaki/daemon.sock`, in a directory
only you can open, and it accepts connections from your user only. Without
`XDG_RUNTIME_DIR` no daemon runs, and every command logs on by itself. After
`daemon_idle_timeout` with nothing to do, the daemon drops its session and
exits; the next command starts a new one.

## Installs

Installs, updates, verifications and Workshop downloads share one download
engine. It picks the least-loaded content servers, spreads chunk requests over
a pool of them, retries a failed chunk on another server, and keeps 16 chunk
requests in flight per depot. It uses HTTPS servers whenever Steam lists
any, and keeps every file a depot names inside the game's directory.
Unfinished installs stay in `install_backlog.toml` until they finish or you
`install forget` them, and a resumed one reuses the files already on disk. `pins.toml` holds the games that
`pin` or `downgrade` hold at a build.
