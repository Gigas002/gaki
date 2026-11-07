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

After the library loads (and after each `r` reload), the installed games are
checked for updates in the background, the same check as `gaki-cli update`
without a game, so pinned games are skipped. It fetches a manifest per
installed game, so it can take a while: the status bar shows
`▲ N updates (checking…)` until it lands, then the `Updates` tab and the `▲`
badges fill in. A failed check (offline, say) keeps the earlier flags and says
so in the status bar. Set `check_updates = false` to turn it off.

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
| `o` / `D` / `L`       | Depots (and their files, see below), DLC, launch options |
| `P`                   | Proton builds (`d` sets the default, `i` installs, `u` uninstalls) |
| `C` / `W` / `a`       | Cloud saves, Workshop, achievements                      |
| `M` / `K` / `N`       | Move, relink, import an install                          |
| `O`                   | Collections                                              |
| `E` / `B` / `T`       | Open the game's Steam store page or ProtonDB page, or search the Proton issue tracker for it |
| `I` / `w`             | Inventory, wallet                                        |
| `m` / `S`             | Market listings, market search                           |
| `F`                   | Focus the Friends panel (`c` chats, `t` opens the chat in a new window, `a` adds, `x` removes) |
| `R`                   | Running games                                            |
| `Y`                   | Store: price, players, news, reviews, similar games      |
| `J`                   | Steam Store: search, deals, events, your wishlist        |
| `X`                   | Installs: running installs and the backlog               |
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

### Installs

`X` lists the installs running in this TUI, then the install backlog, which
`gaki-cli` shares: paused, interrupted and stopped installs, and imports
still queued. Each row shows its state, progress, and why it stopped. The
list refreshes every second.

| Key           | Action                                                         |
| ------------- | -------------------------------------------------------------- |
| `r` / `Enter` | Resume the highlighted entry into the library and platform it started with |
| `R`           | Resume every waiting entry, one after another                  |
| `p` / `s`     | Pause or stop it (a waiting entry is just relabelled)          |
| `f`           | Forget it; its files stay                                      |
| `F`           | Forget it and delete its partial files, when that's provably safe |

Both forgets ask first. A game another gaki process (`gaki-cli`, its
session daemon, another TUI) is downloading shows as `running elsewhere`,
and the keys leave it to that process. See `gaki-cli install` in
[GAKI-CLI.md](GAKI-CLI.md) for the lock behind this.

### Store

With the `store` feature, `Y` (or **Store** in the Actions menu) opens the
selected game's store data in five sections: **Price**, **Players**,
**News**, **Reviews** and **Similar**. `Tab` / `Shift-Tab` (or `h` / `l`)
switch sections, and `j` / `k` and `PgUp` / `PgDn` scroll. A section loads in
the background the first time you show it, and stays loaded until you close
the overlay with `Esc`.

- **Price** shows the price, discount, full price and sale end in each
  region. Set the regions with `store_regions` in `tui/config.toml`;
  with none set, it uses your account's country. `r` edits the list for this
  session (codes separated by commas or spaces).
- **Players** is the number of people in-game right now.
- **News** shows the latest ten announcements, with their links.
- **Reviews** shows the overall score and the ten most helpful reviews.
- **Similar** lists related games, priced for your country.

This is `gaki-cli price --compare`, `players`, `news`, `reviews` and
`similar`.

`J` opens the **Steam Store** overlay for everything not tied to one game,
in four sections. It opens on **Search**, ready to type: `Enter` searches,
and `/` edits the search again. **Deals** starts on the discounted top
sellers; `s` switches to top sellers and then specials (anything on sale).
**Events** lists the store's current sales and events, and **Wishlist** your
own wishlist. `Tab` / `Shift-Tab` switch sections, `j` / `k` move, and `r`
reloads the shown section.

On a game row, `Enter` opens that game's Store overlay (the `Y` one, even
for games you don't own; `Esc` comes back), `E` opens its store page in the
browser, and `w` adds it to your wishlist, or removes it if it's already on
it, after a `y` / `n` confirm. In the Wishlist section, `w` always removes.
This is `gaki-cli search`, `deals`, `events` and `wishlist` (`add`,
`remove`). gaki never buys anything: a wishlist is free.

### Depot files

`o` lists the selected game's depots; the list loads in the background.
With the `depot` feature, `Enter` on a depot opens its files: the current
manifest on the public branch, loaded in the background too.

| Key                   | Action                                                    |
| --------------------- | --------------------------------------------------------- |
| `j` / `k`, `PgUp` / `PgDn` | Move, page                                           |
| `/`                   | Filter as you type: plain text matches anywhere in the path, `*` and `?` are wildcards; `Enter` keeps it, `Esc` clears it |
| `b`                   | Load another branch's current manifest                    |
| `d`                   | Download the highlighted file                             |
| `Esc` / `q`           | Back to the depot list                                    |

`d` asks for a directory, prefilled with the one used last in this session,
or else your Downloads folder (`XDG_DOWNLOAD_DIR`, else `~/Downloads`); `~`
expands. The file is saved there under its own name, checked against the
manifest's SHA-1, and never replaces an existing file. One download runs at a
time. It keeps going if you close the overlay, and the status bar says when
it's done. Only regular files download; this is `gaki-cli depots download`.

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
| `check_updates`  | check the installed games for updates after the library loads (`true`) |
| `store_regions`  | the regions the Store overlay compares prices in, e.g. `["US", "DE", "JP"]`; empty (the default) means your account's country |

The cached library (`games.toml`) and cover art (`icons/`) are in
`$XDG_CACHE_HOME/gaki/tui/`, and the log is
`$XDG_STATE_HOME/gaki/gaki-tui.log`. `--log-level` sets how much it writes.
A `config.json` from an older version, here or in `~/.config/aurelia-tui/`,
isn't read and can be deleted.
