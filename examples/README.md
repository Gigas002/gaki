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

Profiles use the same format as lootrs, so `profiles_dir` in `config.toml` can
point both tools at one directory.

`libgaki/tests/examples.rs` parses and validates every file here.
