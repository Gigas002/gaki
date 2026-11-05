# Examples

Example configuration for gaki. Copy what you need into `~/.config/gaki/`
(or `$XDG_CONFIG_HOME/gaki/`):

| File | Contents |
| --- | --- |
| [`config.toml`](config.toml) | global settings |
| [`games.toml`](games.toml) | per-game overrides, one `[[game]]` per game |
| [`profiles/`](profiles) | one runner, prefix and environment per file; the file name must match `name` |

Every file is optional. Settings apply in this order, later ones winning:
built-in defaults, `config.toml`, the game's profile, its `games.toml` entry,
command-line flags.

`wrappers` (in a profile or a game entry) wrap the launch command, the
profile's outside the game's: `mangohud`, `gamemoderun`, `gamescope -f --`, or
your own script. Each gets the rest of the command as its arguments, so a script
ends with `exec "$@"`; gaki also exports `GAKI_APP_ID`, `GAKI_APP_NAME`,
`GAKI_GAME_DIR`, `GAKI_LAUNCH_PROGRAM` and `GAKI_LAUNCH_ARGS`.

`steam` (in a profile or a game entry) picks the Steam integration: `auto`
bridges to the host Steam client when it is running, `host` always does
(starting it when needed), and `none` never does. `--steam` means `host`.

Profiles use the same format as lootrs, so `profiles_dir` in `config.toml` can
point both tools at one directory.

`libgaki/tests/examples.rs` parses and validates every file here.
