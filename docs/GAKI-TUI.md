# gaki-tui

`gaki-tui` is a terminal UI for browsing, installing, updating and launching
your Steam library. It runs everything in-process through `libgaki`, the same
library behind [`gaki-cli`](GAKI-CLI.md), so the two share a session, a
configuration and a launch pipeline.

```bash
cargo build --release -p gaki-tui
./target/release/gaki-tui
```

Press `?` any time for the keys. The help lists only what your build has: a
build without `social` has no Friends tab, and one without `install` can't
download. [FEATURES.md](FEATURES.md#gaki-tui) has the details.

## Signing in

On startup `gaki-tui` restores your stored session, the same one `gaki-cli
login` creates, and goes straight to your library. Without one it shows a
sign-in page:

- **`Enter`: username and password**, typed in the TUI. If Steam Guard asks
  for a code, you type it on the next screen. The password goes straight to
  `libgaki`'s login and is cleared from the TUI once sent.
- **`y`: QR code**, scanned with the Steam Mobile app. Nothing secret is typed
  anywhere.

You can also sign in with `gaki-cli login` in another terminal, then press `r`
on the sign-in page to check again.

## The library

The window has **tabs** across the top (`All`, `Installed`, `Updates`,
`Favourites`, and `Friends`), the game list with **status badges** (`●`
installed, `▲` update, `⬇` downloading, `○` not installed, `♡` favourite), a
detail pane over the game's art, and a **status bar** with live counts, the
filter and sort, your account and the version.

Selecting a game loads its store details, its ProtonDB tier and its art in the
background, so the list stays responsive while they fill in.

### Keys

| Key                   | Action                                                   |
| --------------------- | -------------------------------------------------------- |
| `j` / `↓`, `k` / `↑`  | Move down / up (the mouse wheel works too)               |
| `g` / `G`             | Jump to the top / bottom                                 |
| `PageUp` / `PageDown` | Page through the list                                    |
| `Tab` / `Shift-Tab`   | Cycle the tabs                                           |
| `1`–`5`               | Jump to a tab (`5` is Friends)                           |
| `/`                   | Filter by name (`Esc` clears)                            |
| `s`                   | Cycle the sort (name, installed first)                   |
| `Enter`               | The Actions menu for the selected game                   |
| `i`                   | Expand or collapse the description                       |
| `d`                   | Install (pick a library)                                 |
| `Space` / `c`         | Pause or resume, or cancel, an install                   |
| `U` / `v` / `x`       | Update, verify, uninstall                                |
| `V` / `b`             | Versions and pinning, beta branches                      |
| `o` / `D` / `L`       | Depots, DLC, launch options                              |
| `P`                   | Proton builds (`d` sets the default, `i` installs, `u` uninstalls) |
| `C` / `W` / `a`       | Cloud saves, Workshop, achievements                      |
| `M` / `K` / `N`       | Move, relink, import an install                          |
| `O`                   | Collections                                              |
| `E` / `B` / `T`       | Open the game's Steam store page or ProtonDB page, or search the Proton issue tracker for it |
| `I` / `w`             | Inventory, wallet                                        |
| `m` / `S`             | Market listings, market search                           |
| `F`                   | Focus the Friends panel (`c` chats, `t` opens the chat in a new window, `a` adds, `x` removes) |
| `R`                   | Running games                                            |
| `f` / `H`             | Toggle a favourite, hide the game                        |
| `A` / `p`             | Account (`o` logs out), settings (presence, proxy)       |
| `r`                   | Refresh the library from Steam                           |
| `l`                   | Sign in again                                            |
| `?`                   | Help                                                     |
| `q`                   | Quit                                                     |

### The Actions menu

`Enter` opens a filterable menu with every action that applies to the selected
game: type to narrow it, `Enter` to run. Two entries live only here:

- **Play as…** lists every way the game can start: the native Linux build, or
  the Windows build, either always bridged to the host Steam client or bridged
  only when Steam is running. The choice applies
  to that launch only, and an unavailable one says what's missing.
- **Game settings** edits the game's `games.toml` entry: `Enter` cycles the
  profile, the platform (auto, windows, linux) and the Steam integration (the
  profile's, auto, host, none). Wrappers are shown read-only; change them with
  `gaki-cli config game <id> --wrapper …`.

### Chat in its own window

`t` on a friend opens the chat in a new terminal window. That runs
`gaki-tui --chat <steamid> --name <name>`, which you can also start yourself.

## Configuration

`gaki-tui` reads gaki's own configuration ([CONFIG.md](CONFIG.md)) for
everything about the session, installs and launches. Its own settings are in
`$XDG_CONFIG_HOME/gaki/tui/config.toml`:

| Key              | Meaning                                                       |
| ---------------- | ------------------------------------------------------------- |
| `favorite_games` | your favourites (`f`)                                         |
| `hidden_games`   | the games `H` hid                                             |
| `allowed_games`  | the app types listed: any of `Game`, `DLC`, `Driver`, `Application`, `Config`, `Demo`, `Tool`, `Unknown`. The default is `Game` and `DLC` |
| `highlight`      | the highlight color                                           |

The cached library (`games.toml`) and cover art (`icons/`) are in
`$XDG_CACHE_HOME/gaki/tui/`, and the log is
`$XDG_STATE_HOME/gaki/gaki-tui.log`. `--log-level` sets how much it writes.
A `config.json` from an older version, here or in `~/.config/aurelia-tui/`,
isn't read and can be deleted.
