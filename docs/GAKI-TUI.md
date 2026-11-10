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

The detail pane also shows your playtime: the total, the last two weeks and
the date you last played.

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

| Key                   | Action                                                                                                                |
| --------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `j` / `↓`, `k` / `↑`  | Move down / up (the mouse wheel works too)                                                                            |
| `g` / `G`             | Jump to the top / bottom                                                                                              |
| `PageUp` / `PageDown` | Page through the list                                                                                                 |
| `Tab` / `Shift-Tab`   | Cycle the tabs                                                                                                        |
| `1`–`5`               | Jump to a tab (`5` is Friends)                                                                                        |
| `/`                   | Filter by name (`Esc` clears)                                                                                         |
| `s`                   | Cycle the sort (name, installed first, most played, recently played)                                                  |
| `Enter`               | The Actions menu for the selected game                                                                                |
| `i`                   | Expand or collapse the description                                                                                    |
| `d`                   | Install (pick a library)                                                                                              |
| `Space` / `c`         | Pause or resume, or cancel, an install                                                                                |
| `U` / `v` / `x`       | Update, verify, uninstall                                                                                             |
| `V` / `b`             | Versions and pinning, beta branches                                                                                   |
| `o` / `D` / `L`       | Depots (and their files, see below), DLC, launch options                                                              |
| `P`                   | Proton builds (`d` sets the default, `i` installs, `u` uninstalls)                                                    |
| `C` / `W` / `a`       | Cloud saves, Workshop, achievements (both below)                                                                      |
| `M` / `K` / `N`       | Move, relink, import an install                                                                                       |
| `O`                   | Collections: `a`/`r` add or remove the game, `n` new, `e` rename, `x` delete (asks), `P`/`U`/`S` cloud pull/push/sync |
| `E` / `B` / `T`       | Open the game's Steam store page or ProtonDB page, or search the Proton issue tracker for it                          |
| `Z`                   | Open the game's SteamDB page (in Depots and Versions: the highlighted depot's manifest history)                       |
| `I` / `w`             | Inventory, wallet                                                                                                     |
| `m` / `S`             | Market listings, market search                                                                                        |
| `F`                   | Focus the Friends panel (`c` chats, `t` opens the chat in a new window, `a` adds, `x` removes)                        |
| `R`                   | Running games                                                                                                         |
| `Y`                   | Store: price, players, news, reviews, similar games                                                                   |
| `n`                   | Game info: details, DRM and files checks, save artwork (see below)                                                    |
| `J`                   | Steam Store: search, deals, events, your wishlist                                                                     |
| `X`                   | Installs: running installs and the backlog, duplicates, library export and import                                     |
| `f` / `H`             | Toggle a favourite, hide the game (Steam's own collections, see below)                                                |
| `A` / `p`             | Account (`o` logs out), settings (see below)                                                                          |
| `r`                   | Refresh the library from Steam                                                                                        |
| `l`                   | Sign in again                                                                                                         |
| `?`                   | Help                                                                                                                  |
| `q`                   | Quit                                                                                                                  |

### The Actions menu

`Enter` opens a filterable menu with every action that applies to the selected
game: type to narrow it, `Enter` to run. Three entries live only here:

- **Pre-cache shaders** fetches Steam's pre-built shaders for the game and
  compiles them (`gaki-cli shaders`; see
  [Shader pre-caching](CONFIG.md#shader-pre-caching)), with the outcome in
  the game's status. It compiles even when nothing changed, for after a
  driver update. Installs and updates do this on their own while the status
  says `pre-caching shaders...`, unless the Settings tab turns
  **Shader pre-caching** off.
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

| Key           | Action                                                                     |
| ------------- | -------------------------------------------------------------------------- |
| `r` / `Enter` | Resume the highlighted entry into the library and platform it started with |
| `R`           | Resume every waiting entry, one after another                              |
| `p` / `s`     | Pause or stop it (a waiting entry is just relabelled)                      |
| `f`           | Forget it; its files stay                                                  |
| `F`           | Forget it and delete its partial files, when that's provably safe          |

Both forgets ask first. A game another gaki process (`gaki-cli`, its
session daemon, another TUI) is downloading shows as `running elsewhere`,
and the keys leave it to that process. See `gaki-cli install` in
[GAKI-CLI.md](GAKI-CLI.md) for the lock behind this.

`Tab` switches to **Duplicates**, which is `gaki-cli duplicates`: each game
installed in more than one Steam library, the copy gaki keeps (the most
recently updated), the stale copies with their sizes, and the space deleting
them would free. Two copies leave two appmanifests disagreeing, which is why
such a game can report an update on every check. The tab is read-only: gaki
never deletes a copy, so remove the stale folder yourself. The scan runs the
first time the tab is shown; `r` scans again.

The next tab, **Transfer**, is `gaki-cli library export` and `import`, to
rebuild a library on another machine. The file starts as
`~/gaki-library.toml`; `e` edits the path (`~` works), `Enter` finishes.

| Key | Action                                                                                    |
| --- | ----------------------------------------------------------------------------------------- |
| `a` | Export only the installed games (the default) or every game in the library                |
| `x` | Export: write the file. Installed games carry their platform and beta branch              |
| `i` | Import: read the file and list each game as install, skip (installed) or skip (not owned) |

When an import has games to install, `y` queues them in the backlog and
installs them one after another into the configured library, and the overlay
switches back to the queue; `n` drops the list. Installs take the default
branch, so a game that was on a beta branch is marked: switch it afterwards
in Versions (`V`) and update it.

### Settings

`p` lists every `config.toml` key with its value, or its default in
brackets when it isn't set. `j` / `k` move; the line under the list says
what the highlighted key is and what an unset one means. `Enter` edits it:
a switch flips, a choice steps to the next value (the default profile
steps through your profiles), and anything else opens a prompt with the
current value. `x` unsets the key, back to its default. Saving checks the
value like `gaki-cli config set`, so a bad one is refused with the reason.
Changing where the session tokens are kept (`secret_store`) asks first. A
few keys apply later, and the status line says when: the proxy when gaki next
starts, `daemon_idle_timeout` when the daemon next starts, and the library
and profile directories after a library reload (`r`).

`Tab` / `Shift-Tab` switch to three more tabs:

- **Profiles** lists the profiles, the default one marked. Beside the
  highlighted profile it shows its file, its issues, the games whose
  `games.toml` entry names it, and its settings as they read in the file
  (`PgUp` / `PgDn` scroll them). `d` makes it the default profile. Profile
  files themselves are edited by hand.
- **Check** runs `gaki-cli config check` each time you open the tab: files
  that didn't load, then errors, then warnings, or "No problems found."
  `r` checks again.
- **Daemons** lists the running `gaki-cli` session daemons, each with its
  PID, config directory and socket (`(default)` when it uses the usual
  one), and the highlighted one's full command. `s` stops the highlighted
  daemon and `S` stops them all, each after a y/n; this is
  `gaki-cli daemon stop`. A client starts a fresh daemon the next time it
  needs one. `r` reads the list again.
- **Clean** looks for gaki's caches and leftovers each time you open the
  tab and lists each kind with its size, as `gaki-cli clean` does. `x`
  deletes them all after a y/n, `a` switches between only the shader
  caches of uninstalled games and every game's, and `r` looks again.

### Cloud saves

`C` lists the game's Steam Cloud files. `s` syncs both ways, `d` only
downloads, `u` only uploads; each runs in the background. A sync never
overwrites a save that changed on both sides: it lists those with each
side's size and modification time (UTC), the newer side marked. `c` then
keeps the cloud copies (overwriting the local ones) and `l` keeps the local
copies (overwriting the cloud ones), both after a `y`/`n`. This is
`gaki-cli cloud sync --resolve cloud|local`, for every diverged save of the
game at once.

### Workshop

With the `workshop` feature, `W` lists the selected game's installed and
subscribed Workshop items: `●` installed, `○` subscribed only, with
`update available` when Steam has newer content than what's on disk. The
list loads in the background.

| Key | Action                                        |
| --- | --------------------------------------------- |
| `i` | Install the highlighted item, or update it    |
| `u` | Uninstall it (asks first)                     |
| `U` | Update every outdated item, one after another |
| `r` | Reload the list                               |
| `b` | Browse and search the game's Workshop         |

In browse, typing goes to the search line, so the actions use other keys:
`Enter` searches, `↑` / `↓` move, `Tab` subscribes or unsubscribes, `F1` /
`F2` rate up or down, `F3` opens the comments, and `F4` installs (a
collection installs its items). In the comments, `c` writes a comment:
`Enter` asks before it posts, since comments are public.

One install or uninstall runs at a time. It keeps going if you close the
overlay, and its progress shows again when you reopen it. This is
`gaki-cli workshop status`, `install`, `uninstall` and `comment`.

### Game info

`n` (or **Game info** in the Actions menu) shows the selected game in three
tabs, switched with `Tab`. Each loads in the background; `j` / `k` and
`PgUp` / `PgDn` scroll.

- **Details** is the whole store record, as `gaki-cli info --extended`
  shows it: developers and publishers, release date, price in your
  country, platforms, reviews, age rating, tags, genres and categories,
  Metacritic, links, what it's sold as, system requirements and the full
  description.
- **Checks** asks Steam for the game's DRM tickets (`gaki-cli drm`: the
  ownership ticket and the encrypted app ticket) and looks at its files on
  disk (`gaki-cli available`: installed, where, pinned, and the size of its
  shader cache). They run the first
  time you show the tab; `r` runs them again.
- **Artwork** lists the store and library images and saves them with `s`
  as `<app id>_<kind>.<ext>`, into your Pictures folder by default (`o`
  changes it for this session). A file that already exists is never
  overwritten; that image is reported and the others still save.

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

| Key                        | Action                                                                                                                    |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `j` / `k`, `PgUp` / `PgDn` | Move, page                                                                                                                |
| `/`                        | Filter as you type: plain text matches anywhere in the path, `*` and `?` are wildcards; `Enter` keeps it, `Esc` clears it |
| `b`                        | Load another branch's current manifest                                                                                    |
| `d`                        | Download the highlighted file                                                                                             |
| `Esc` / `q`                | Back to the depot list                                                                                                    |

`d` asks for a directory, prefilled with the one used last in this session,
or else your Downloads folder (`XDG_DOWNLOAD_DIR`, else `~/Downloads`); `~`
expands. The file is saved there under its own name, checked against the
manifest's SHA-1, and never replaces an existing file. One download runs at a
time. It keeps going if you close the overlay, and the status bar says when
it's done. Only regular files download; this is `gaki-cli depots download`.

### User profiles

In the Friends tab, `v` shows the highlighted friend's public profile: the
fields `gaki-cli user` prints (persona name, SteamID, profile URL, privacy,
name, location, account age, status and current game, last seen, headline,
summary and any community ban). `/`, in the Friends tab or the profile
overlay, looks anyone up by SteamID64, profile URL, a friend's name or `me`.
`E` opens the profile page in your browser; `j` / `k` scroll.

### Chat in its own window

`t` on a friend opens the chat in a new terminal window. That runs
`gaki-tui --chat <steamid> --name <name>`, which you can also start yourself.

## Configuration

`gaki-tui` reads gaki's own configuration ([CONFIG.md](CONFIG.md)) for
everything about the session, installs and launches. Its own settings are in
`$XDG_CONFIG_HOME/gaki/tui/config.toml`. gaki-tui saves this file itself
(after a login, and when favourites are migrated), keeping its comments and
layout, so it can be edited by hand:

| Key             | Meaning                                                                                                                                   |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `allowed_games` | the app types listed: any of `Game`, `DLC`, `Driver`, `Application`, `Config`, `Demo`, `Tool`, `Unknown`. The default is `Game` and `DLC` |
| `highlight`     | the highlight color                                                                                                                       |
| `check_updates` | check the installed games for updates after the library loads (`true`)                                                                    |
| `store_regions` | the regions the Store overlay compares prices in, e.g. `["US", "DE", "JP"]`; empty (the default) means your account's country             |

Favourites (`f`, the Favourites tab) and hidden games (`H`) are Steam's
own built-in Favorites and Hidden collections, the same ones as in the
Steam client and `gaki-cli collections`. A change is saved in the local
collections store at once and synced to Steam in the background; if that
fails (offline, signed out) the status bar says so and the next sync sends
it. Steam's collections are fetched after the library loads, so marks made
in the Steam client show up. To unhide a game, remove it from Hidden in the
collections overlay or with `gaki-cli collections remove hidden <app>`.
An older `config.toml` with `favorite_games` or `hidden_games` lists has
them moved into those collections on the first start, and the keys are
dropped. A build without the `collections` feature keeps them as lists in
`config.toml` instead.

The cached library (`games.toml`) and cover art (`icons/`) are in
`$XDG_CACHE_HOME/gaki/tui/`, and the log is
`$XDG_STATE_HOME/gaki/gaki-tui.log`. `--log-level` sets how much it writes.
A `config.json` from an older version, here or in `~/.config/aurelia-tui/`,
isn't read and can be deleted.

### Key bindings

Keys can be changed in `$XDG_CONFIG_HOME/gaki/tui/keybinds.toml`, a file
of its own, never `config.toml`. It has one section per context: `[library]`
for the game list with no overlay open, `[overlay]` for the keys every
overlay shares (`close`, `down`, `up`, `page_down`, `page_up`, `next_tab`,
`previous_tab`), and a section for each overlay with its own keys:
`[installs]`, `[settings]`, `[workshop]`, `[workshop_browse]`,
`[workshop_comments]`, `[store]`, `[steam_store]`, `[depots]`,
`[depot_files]`, `[game_info]`, `[cloud]`, `[collections]`, `[versions]`,
`[launch_targets]`, `[game_settings]`, `[install_picker]`, `[dlc]`,
`[branches]`, `[running]`, `[proton]`, `[account]`, `[market_search]`,
`[user_profile]`, plus `[friends]` for the Friends tab (its moving and
leaving come from `[overlay]`, its `Tab` from `[library]`) and
`[actions_menu]` (`run`, and `next` / `previous`, unbound by default).
Overlays with nothing of their own (achievements, inventory, launch
options, market listings, the help) use `[overlay]` alone. Each line binds
an action to a key or a list of keys; an empty list leaves it without one
(the Actions menu still has the library's):

```toml
[library]
install = "i"
description = "d"
sort = ["s", "ctrl+s"]
hide = []

[overlay]
close = ["q", "ctrl+w"]

[installs]
pause = "z"
```

Keys are one character, case-sensitive (`"E"` is Shift+e), or a name:
`Enter`, `Esc`, `Tab`, `BackTab` (Shift+Tab), `Backspace`, `Delete`,
`Insert`, `Home`, `End`, `PageUp`, `PageDown`, the arrows, `Space`,
`F1`–`F24`, each optionally after `ctrl+` and/or `alt+`. An action left out
keeps its default, so a missing or empty file changes nothing.
[`examples/keybinds.toml`](../examples/keybinds.toml) lists every section
and action with its default key.

An overlay's own keys can't clash with `[overlay]`'s. Some belong to one tab
(the example marks them), and actions on different tabs may share a key:
`r` resumes in the Installs queue and rescans in Duplicates. The sections
that take typed text, `[workshop_browse]`, `[market_search]` and
`[actions_menu]`, only take keys that don't type (`F5`, `ctrl+s`, `Tab`),
and `[overlay]` doesn't apply there; their `Enter` and `Backspace` stay
put, except the menu's `Enter`, which is `run`.

The file is read once at startup. On an unknown section or action, a key it
can't read, or two actions on one key, gaki-tui doesn't start and prints the
file and line, e.g.
``keybinds.toml:3: `x` is bound to both `sort` and `uninstall` in [library]``.
`Esc`, `?` (help), the arrow keys and Ctrl+C (quit, from anywhere) can't be
rebound, so a bad file can't lock you out; text prompts and y/n questions
keep their keys too, as do the add-friend, move, relink and import prompts
and the library's text filter. The help (`?`), the status-bar hints, the
Actions menu and the overlays' footers and titles show the keys in effect;
the help lists what each library key opens, and each overlay's footer its
own keys.
