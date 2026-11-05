# gaki-tui

A terminal UI for browsing, installing, updating, and launching your Steam
library. It's part of [gaki](../README.md) and runs everything in-process
through `libgaki`, the same library behind `gaki-cli`.

## About

gaki-tui began as [Aurelia-TUI](https://github.com/Drackrath/Aurelia-TUI), a
rebuild of [`steam-tui`](https://github.com/dmadisetti/steam-tui) that drove the
`aurelia` CLI as a subprocess and parsed its JSON output. gaki-tui calls
`libgaki` directly instead: authentication, library listing, downloads, and
launching (including Proton) all run inside the TUI, with no second binary to
install.

## Requirements

- An installed official Steam client. gaki works alongside it rather than
  replacing it.
- A Steam account (logging in is described below).

## Building

It's a standard Rust project:

```bash
cargo build --release
./target/release/gaki-tui
```

## Usage

Launch `gaki-tui` and you're off — the in-app help bar always shows the keys
for the current screen.

On startup it restores your stored session, the same one `gaki-cli login`
creates. If you're already signed in, it goes straight to your library.

### Signing in

If no session is found, you get a sign-in screen with two options:

- **`Enter` — username & password**, entered inside the TUI. If Steam Guard asks
  for a code, you type it on the next screen. The password goes straight to
  `libgaki`'s login and is cleared from the TUI once sent. Use QR if you'd rather
  never type it here at all.
- **`y` — QR code**, scanned with the Steam Mobile app. Nothing secret is typed
  anywhere.

Either path loads your library automatically once you authenticate. You can also
log in out-of-band with `gaki-cli login` in a terminal, then press `r` on the
sign-in screen to re-check the session.

### Browsing

The library is a single live browser: **filter tabs** across the top
(`All · Installed · Updates · Favourites`, plus a `Friends` tab), the game list
with **status badges** (`●` installed, `▲` update, `⬇` downloading, `○` not
installed, `♡` favourite), a detail pane with cover art, and a **status bar**
showing live counts, the active filter/sort, your account, and the gaki-tui
version. Press `?` any time for the full key list.

#### Keys

| Key | Action |
| --- | --- |
| `j` / `↓`, `k` / `↑` | Move down / up (mouse wheel works too) |
| `g` / `G` | Jump to top / bottom |
| `PageUp` / `PageDown` | Page through the list |
| `Tab` / `Shift-Tab` | Cycle tabs |
| `1`–`5` | Jump straight to a tab (`5` = Friends) |
| `/` | Focus the live fuzzy filter (type to narrow, `Esc` clears) |
| `s` | Cycle sort (name / installed-first) |
| `Enter` | Open the Actions menu for the selected game |
| `i` | Expand / collapse the description panel |
| `d` | Install / download (pick a library) |
| `Space` / `c` | Pause / resume, or cancel, an install |
| `U` / `v` / `x` | Update, verify, or uninstall |
| `V` | Versions & pinning (downgrade to a manifest) |
| `b` / `o` / `D` | Beta branches, depots, DLC |
| `P` | Proton runtimes |
| `C` / `W` / `a` / `I` | Cloud saves, Workshop, achievements, inventory |
| `M` / `K` / `N` / `O` | Move, relink, import an install; collections |
| `m` / `S` / `w` | Market listings, market search, wallet |
| `R` | Running games |
| `f` / `H` | Toggle favourite, hide the game |
| `A` / `p` | Account, settings (presence, proxy) |
| `r` | Refresh the library from Steam |
| `l` | Sign in again |
| `?` | Toggle the help overlay |
| `q` | Quit |

### The Actions menu

`Enter` opens a filterable command palette with every action that applies to the
selected game — type to narrow it, `Enter` to run. Two entries live only here:

- **Play as…** lists every way the game can be started: the native Linux build, or the Windows build with the host
  Steam client or without Steam. Launching one applies to that
  run only; unavailable targets say what's missing.
- **Game settings** edits the game's `games.toml` entry: `Enter` cycles the
  profile (default, then each profile), the platform (auto / windows / linux)
  and the Steam integration (the profile's / auto / host / none). The
  wrappers are shown read-only; change them with
  `gaki-cli config game <id> --wrapper …`.

Selecting a game fetches its details lazily — developer/publisher metadata
(Steam store info), the ProtonDB compatibility tier, and cover art all load in the
background, so navigation stays responsive while they fill in.

## Features not in the help

A little Easter egg for reading the documentation.

### Favourites

`f` toggles a favourite (shown with a ♡), and the Favourites tab (`4`) filters
the list down to just your favourites.

### Hiding games

`H` hides the selected game. Hidden games are recorded in
`~/.config/gaki/tui/config.json`.

### Showing other app types (demos, tools, …)

By default only `Game` and `DLC` entries are shown. Edit the `allowed_games`
field in the config to include any of `Game`, `DLC`, `Driver`, `Application`,
`Config`, `Demo`, `Tool`, or `Unknown`.

## Configuration & data

| Path | Contents |
| --- | --- |
| `$XDG_CONFIG_HOME/gaki/tui/` (`~/.config/gaki/tui/`) | `config.json` (favourites, hidden games, allowed types, highlight colour) |
| `$XDG_CACHE_HOME/gaki/tui/` (`~/.cache/gaki/tui/`) | cached library (`games.json`) and cover art (`icons/`) |
| `$XDG_STATE_HOME/gaki/` (`~/.local/state/gaki/`) | `gaki-tui.log` |

A `config.json` left in the old `~/.config/aurelia-tui/` is copied over on
the first start. The old directories can be deleted afterwards.

Launching and Proton are handled by `libgaki`, the same code as `gaki-cli play`,
so there are no wrapper scripts or separate runtimes to manage on your side.

## Credits

- [steam-tui](https://github.com/dmadisetti/steam-tui) — the original client this
  is rebuilt from, by [@dmadisetti](https://github.com/dmadisetti) and contributors
- [Aurelia-TUI](https://github.com/Drackrath/Aurelia-TUI) — the TUI gaki-tui is
  forked from, built on [Aurelia](https://github.com/Drackrath/Aurelia)
- [steam-vent](https://codeberg.org/steam-vent/steam-vent) — Steam protocol implementation

## License

Distributed under GPL-3.0-only. See [`LICENSE`](../LICENSE) for details.
