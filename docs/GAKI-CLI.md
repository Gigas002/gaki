# gaki-cli

`gaki-cli` is gaki's command-line client: log in, manage your library, install
and update games, sync Steam Cloud saves, and launch titles natively or through
Proton, from a terminal or a script. `gaki-cli --help` lists every command, and
`gaki-cli <command> --help` explains one.

Some commands need a Cargo feature that default builds leave out (`store`,
`social`, `market`, ...). [FEATURES.md](FEATURES.md) says which, and
`gaki-cli capabilities` lists what your build has.

## Commands

A game can be named by its app id, or by the `name` alias its
[`games.toml`](CONFIG.md#gamestoml) entry gives it.

### Account

```bash
gaki-cli login                        # sign in (prompts for the password and Steam Guard)
gaki-cli login --qr --timeout 120     # scan a QR code with the Steam Mobile app; give up after 120 s
gaki-cli logout                       # revoke the token at Steam and clear the stored session
gaki-cli account                      # show account details
```

### Library

```bash
gaki-cli list                         # your library, with playtime and last-played date
gaki-cli list --sort playtime         # most played first (also: recent, name)
gaki-cli list --installed             # only installed games
gaki-cli list --search elden          # filter by name
gaki-cli list --online                # add an ONLINE column (needs-connection heuristic)
gaki-cli list --check-updates         # compare installed manifests with Steam's
gaki-cli info 690830                  # details: description, release, reviews, tags, DLC
gaki-cli info 690830 --extended       # + requirements, Metacritic, genres, categories
gaki-cli info 690830 --country DE     # quote the price for a region
gaki-cli dlc 690830                   # a game's DLC, with ownership and install status
gaki-cli drm 690830                   # verify DRM and ownership tickets headlessly
gaki-cli achievements 620             # your achievements for a game (unlock state, rarity)
gaki-cli image 1245620                # fetch cover art to the cache (prints the path)
gaki-cli image 1245620 -o cover.jpg   # save artwork to a file
gaki-cli image 1245620 --list --probe # every store asset URL, HEAD-checked
gaki-cli depots 1245620               # list depots
gaki-cli launch-options 1245620       # Steam launch configs (exe, args, platform)
gaki-cli libraries                    # Steam library folders and their free space
```

### Moving a library to another machine

```bash
gaki-cli library export library.toml          # the installed games, as [[game]] entries
gaki-cli library export library.toml --all    # every game in the library
gaki-cli library export                       # print it instead
gaki-cli library import library.toml --dry-run  # what would be installed and skipped
gaki-cli library import library.toml          # install what isn't installed yet
gaki-cli library import library.toml --library /mnt/games/SteamLibrary
```

An entry holds the `app_id`, the `name`, the installed `platform` and a beta
`branch`; only `app_id` is required
([`examples/library.toml`](../examples/library.toml)). `import` skips games
that are installed or not in this account's library. It queues the rest in
the install backlog and installs them one at a time, so `install pause` and
`install stop` work on each. If the import stops, `install resume --all`
installs the remaining games. Installs use the default branch. When an entry
was on a beta branch, `import` prints the `set-branch` and `update` commands
that switch it.

### Store

```bash
gaki-cli search "no man's sky"        # find app ids by title
gaki-cli price 690830 --compare US,DE,JP  # a game's price and discount across regions
gaki-cli deals                        # discounted top sellers in your region
gaki-cli similar 275850               # games Steam considers similar
gaki-cli players 730                  # current player count
gaki-cli events                       # store-wide sales and events running now
gaki-cli news 275850                  # recent announcements
gaki-cli reviews 275850 --type negative  # user reviews, paged with --cursor
gaki-cli wishlist                     # your wishlist with prices (or a friend's: `wishlist <user>`)
gaki-cli wishlist add 1245620         # add an item (`wishlist remove` takes it out)
```

### Install and maintain

```bash
gaki-cli install 1245620              # download and install a game
gaki-cli install 1245620 --library /mnt/games/SteamLibrary  # onto a specific library
gaki-cli install list                 # running installs and the backlog
gaki-cli install pause 1245620        # pause an install, keeping its partial files
gaki-cli install resume 1245620       # resume a paused or interrupted install
gaki-cli install resume --all         # resume every backlog entry, one at a time
gaki-cli install stop 1245620         # cancel a running install; it stays in the backlog
gaki-cli install forget 1245620       # drop a backlog entry (--delete-files removes its files)
gaki-cli update 1245620               # download the latest manifest
gaki-cli verify 1245620               # verify installed files
gaki-cli uninstall 1245620            # remove a game and its shader cache (--delete-prefix also wipes its prefix)
gaki-cli move 1245620 /mnt/games/SteamLibrary    # move an install to another library
gaki-cli relink 1245620 /mnt/games/SteamLibrary  # point Steam at already-moved files
gaki-cli import 1245620 /mnt/games/SteamLibrary  # register files already on disk
gaki-cli available 1245620            # installed and present on disk? (and its shader cache's size)
gaki-cli shaders 1245620              # fetch Steam's pre-built shaders and compile them (--download-only skips compiling)
gaki-cli duplicates                   # games installed in several libraries at once
gaki-cli enable 2001                  # enable an installed DLC
gaki-cli disable 2001                 # disable it
```

Each running install, update or verify holds a lock on
`$XDG_STATE_HOME/gaki/installs/<app_id>.lock`, which the OS releases when its
process exits (crashes included). So `gaki-cli` and `gaki-tui` never run the
same game's download twice. `install list` shows a game another gaki
process is downloading as `running elsewhere`, and `install resume`,
`forget` and `stop` refuse it there; stop it from the process that runs it.

A game installed in two libraries reports an update forever. `duplicates`
prints the copies to delete and removes nothing itself.

After an install or update, gaki fetches Steam's pre-built shaders for the
game and compiles them (`shaders` does it on demand; `shader_precache =
false` turns the automatic run off; see
[Shader pre-caching](CONFIG.md#shader-pre-caching)). With `--json`, the
`install` and `update` results carry the report as `shaders` (`null` when it
didn't run): each bucket's `kind`, `manifest_id` and `status`
(`downloaded`, `current`, `not_offered`, `skipped`, `failed`), and the
compile step's `status` (`compiled` with `graphics` and `compute` counts,
`up_to_date`, `no_pipelines`, `no_replayer`, `skipped`, `failed`). A
failure there never fails the install.

### Versions and branches

```bash
gaki-cli branches 1245620             # beta branches
gaki-cli set-branch 1245620 beta      # switch branch
gaki-cli manifests 1245620            # each depot's current manifest per branch
gaki-cli downgrade 1245620 --depot 1245621 --manifest 8593343465227540543  # install an older build and pin it
gaki-cli pin 1245620                  # hold the current install (updates skip it)
gaki-cli unpin 1245620                # release the pin
```

Steam doesn't list older manifest ids. SteamDB does:
`https://steamdb.info/depot/<depot_id>/manifests/` (`manifests` prints the
link for each depot). gaki never fetches SteamDB itself; the links are only
for you to follow.

### Depot browser (`depot` feature)

```bash
gaki-cli depots files 1245620 1245621                 # every file of the public manifest
gaki-cli depots files 1245620 1245621 -f '*.exe'      # only paths matching a glob
gaki-cli depots files 1245620 1245621 -b beta         # the beta branch's current manifest
gaki-cli depots files 1245620 1245621 -m 8593343465227540543  # a specific (older) manifest
gaki-cli depots download 1245620 1245621 Game/bin/game.exe    # save ./game.exe
gaki-cli depots download 1245620 1245621 Game/bin/game.exe -o ~/old -m 8593343465227540543
```

`files` lists a manifest's entries with their size: `/` marks a directory,
`*` an executable and `->` a symlink's target. `--json` adds each entry's
SHA-1 and chunk count. You need access to the depot (its decryption key), so
in practice you must own the game.

`download` fetches one regular file into the output directory (default: the
current one) under its own name, so `Game/bin/game.exe` becomes `game.exe`.
It refuses an existing file unless you pass `--force`. The file is written
next to its target first, checked against the manifest's SHA-1, and only then
moved into place, so a failed download leaves nothing behind. Paths that
climb out with `..` are refused, the same check an install makes.

### Launch

```bash
gaki-cli play 1245620                 # launch a game and wait for it to exit
gaki-cli play 1245620 --proton experimental  # force a Proton build for this launch
gaki-cli play 1245620 --platform linux       # run the native build (or `windows` through Proton)
gaki-cli play 1245620 --steam         # bridge to the host Steam client (Steamworks, DRM)
gaki-cli play 1245620 --no-update     # skip the pre-launch update
gaki-cli launch-targets 1245620       # the installed builds and Steam modes, with their `play` flags
gaki-cli running                      # games gaki is running
gaki-cli stop 1245620                 # stop a game (--force kills a hung one)
```

How a game runs (its runner, prefix, environment, wrappers and Steam
integration) comes from its profile; see [CONFIG.md](CONFIG.md). `play`
flags override it for one launch.

`play` (and gaki-tui's play) first asks Steam whether the signed-in account
owns the game or has it through Steam Family, and refuses it otherwise
(`access_denied` with `--json`). Installed games of other accounts on this
machine are listed, as not owned, but don't launch. Offline, gaki's cached
library for the account decides, and Family-Shared games need Steam online.

### Proton

```bash
gaki-cli proton list                  # official Proton builds, and what's installed
gaki-cli proton install "Proton 9.0"  # install an official build through Steam
gaki-cli proton default "Proton 9.0"  # the build a Windows launch uses when its profile names none
gaki-cli proton uninstall "Proton 9.0"
gaki-cli config protons               # every runtime gaki detects, custom builds included
```

gaki installs only official builds. Install custom ones (GE-Proton,
proton-cachyos, ...) with your package manager and point a profile's
`proton.build` at them.

### Steam Cloud

```bash
gaki-cli cloud list 1245620           # a game's Cloud files
gaki-cli cloud sync 1245620           # sync down, then up
gaki-cli cloud sync 1245620 --down    # only download
gaki-cli cloud sync 1245620 --resolve cloud  # settle diverged saves by taking the cloud's copy
```

### Collections

Edit collections locally, offline, and sync them with Steam when you choose.

```bash
gaki-cli collections list             # every collection, with game counts
gaki-cli collections create "RPGs"    # a new static collection
gaki-cli collections add "RPGs" 570 730
gaki-cli collections remove "RPGs" 730
gaki-cli collections show "RPGs"
gaki-cli collections rename "RPGs" "Role-playing"
gaki-cli collections delete "Role-playing"  # favourite and hidden can't be deleted
gaki-cli collections add favorite 570 # the built-in ids work even before a pull
gaki-cli list --collection "RPGs"     # filter the library to one collection
gaki-cli collections pull             # fetch Steam's collections and merge them in
gaki-cli collections push --yes       # upload local collections to your account
gaki-cli collections sync --yes       # pull, then push
```

### Steam Workshop

```bash
gaki-cli workshop browse 1245620      # discover items (search, sort, paginate)
gaki-cli workshop info 1234567890     # item or collection metadata
gaki-cli workshop install 1234567890  # download an item (collections expand to members)
gaki-cli workshop subscribe 1234567890 --install  # subscribe, then download
gaki-cli workshop status 1245620      # installed vs subscribed, with update detection
gaki-cli workshop list 1245620        # the items you're subscribed to
gaki-cli workshop uninstall 1234567890  # remove an installed item
gaki-cli workshop unsubscribe 1234567890
gaki-cli workshop rate 1234567890 up  # thumbs up (or down)
gaki-cli workshop comments 1234567890 # read an item's comments
gaki-cli workshop comment 1234567890 "Nice mod!"
```

### Friends and chat

```bash
gaki-cli friends list                         # name, status, current game
gaki-cli friends search "AI Jungle Wulf"      # a SteamID64, profile URL, friend name or `me`
gaki-cli friends add 76561197960287930        # send a friend request
gaki-cli friends remove 76561197960287930     # remove a friend or cancel a request
gaki-cli user me                              # your profile as Steam sees it
gaki-cli chat send 76561198042323314 "hi!"    # send a direct message
gaki-cli chat history 76561198042323314       # recent messages with a friend
gaki-cli chat open 76561198042323314          # live chat (type to send, Ctrl-D quits)
```

`chat open --json` reads lines to send from stdin, so another program can
drive it. Steam's rules forbid bots: a script that sends or answers
messages for you is automation under the Subscriber Agreement
([STEAM-RULES.md](STEAM-RULES.md#your-part)).

### Inventory and market

Read-only, by design: you can look, but never buy, sell or trade. gaki will
never move real money or items.

```bash
gaki-cli inventory 753 --context 6            # your Steam cards, gems and backgrounds
gaki-cli wallet                               # your Steam Wallet balance
gaki-cli market price 440 "Mann Co. Supply Crate Key"  # an item's price (no login needed)
gaki-cli market search "Sticker" --app-id 730 # search the Community Market
gaki-cli market listings                      # your listings and buy orders
```

With `reauth_sensitive` on, the inventory, wallet and listings ask for a fresh
sign-in first ([CONFIG.md](CONFIG.md#session-and-security)).

Steam limits these pages hard, so gaki goes easy on them. A price or search
result is reused for 5 minutes, and your inventory, wallet and listings for
1 minute. Requests are sent at least 3 seconds apart, so a burst of
commands waits its turn. The answers are kept in memory only. Commands
that run through the session daemon share them; with `--no-daemon`, each
command starts empty. Scripts that poll the market are the kind of
automation Steam's rules forbid ([STEAM-RULES.md](STEAM-RULES.md)).

### Configuration

```bash
gaki-cli config show                  # the resolved configuration
gaki-cli config check                 # validate config.toml, the profiles and games.toml
gaki-cli config get default_profile   # one config.toml key, as written (nothing when unset)
gaki-cli config set default_profile proton  # set a config.toml key
gaki-cli config unset default_profile # back to its default
gaki-cli config proxy socks5://127.0.0.1:1080  # proxy for web, CDN and Proton lookups (not the CM)
gaki-cli config info-cache-ttl 3600   # how long `info` results are cached (0 turns it off)
gaki-cli config experimental true     # unlock `login --openid` and `login --web-token`
gaki-cli config presence online       # appear online for chat (default: offline)
gaki-cli config install-resume auto   # resume interrupted installs when the daemon starts
gaki-cli config game 292030 --profile proton-cachyos  # one game's games.toml entry
gaki-cli config game 292030 --wrapper mangohud --wrapper ~/bin/wrap.sh
gaki-cli config game 292030 --steam none
gaki-cli config clear-games           # remove every games.toml entry (asks first)
gaki-cli profiles list
gaki-cli profiles show proton
```

[CONFIG.md](CONFIG.md) covers every file and key.

### Cleaning up

```bash
gaki-cli clean                        # list caches and leftovers with sizes, then ask
gaki-cli clean --dry-run              # only list
gaki-cli clean --logs --locks --yes   # just these kinds, without asking
gaki-cli clean --all-shaders          # also the installed games' shader caches
```

`clean` finds the cover art and artwork cache (`--images`), the `info`
cache and store name tables (`--info`), the cached library (`--library`),
gaki-tui's cache (`--tui`), launch logs other than each game's newest
(`--logs`; one without a summary is kept for a day, in case its game is
still running), install lock files no process holds (`--locks`), and the
shader caches of games that are no longer installed (`--shaders`). With no
kind flag it takes them all; `--all-shaders` adds the installed games'
shader caches, which rebuild on the next launch. A shader cache counts only
when it has the folders gaki or Mesa make, so a numbered folder of your
own under `shader_cache` is left alone. Cache directories are emptied
rather than removed, so a running daemon or TUI keeps working. It never
touches the config, the session, the install backlog, installed games,
Cloud saves, prefixes or partial downloads (`install forget
--delete-files` handles those). `--json` lists the items and the bytes
freed; it needs `--yes` unless it's a `--dry-run`.

## Scripting with `--json`

Add `--json` to any command for machine-readable output, errors included. It's
a global flag, so `gaki-cli --json <command>` and `gaki-cli <command> --json`
are the same.

Drivers can feature-detect with `gaki-cli --json capabilities`. It prints one
line with the version, the JSON `schema` number, the build's Cargo features,
every command with its flags, arguments and subcommands, every `event` name,
every error `type` with its exit code and hint, and behavior `features` that
flags don't reveal. Add a command path (`gaki-cli capabilities config game`) for
one command's arguments, subcommands and flags in detail. It runs offline and
never starts the daemon.

`list --json` and `info --json` (and each DLC in it) carry ready-made links
next to the artwork: `store_url` (the Steam store page) and `steamdb_url`
(the SteamDB page).

Every `{"event":"result",...}` line also carries `"schema"`. It goes up only
when an existing field changes meaning or is removed; new fields don't change
it.

### QR login

`login --qr --json` streams `qr_challenge` and `qr_scanned` events on stderr
and ends with exactly one `{"event":"result",...}` line on stdout. On success a
one-line `{"logged_in":true,...}` object comes just before it. stdin is the
driver's control channel: send a `cancel` line to stop, and keep stdin open
while waiting, because EOF ends the login.

| `type`          | Meaning                                               | Exit code |
| --------------- | ----------------------------------------------------- | --------- |
| `approved`      | Signed in (`ok: true`, with `account` and `steam_id`) | 0         |
| `login_denied`  | Declined in the app, or Steam dropped the session     | 80        |
| `login_expired` | Scanned, but not approved before the code expired     | 81        |
| `timeout`       | No approval within `--timeout` (default 600 s)        | 82        |
| `cancelled`     | The driver sent `cancel`                              | 83        |
| `input_closed`  | stdin reached EOF                                     | 84        |

### Progress

`install`, `update` and `verify` stream `progress` events on stderr.
`bytes_downloaded` includes `reused_bytes`, the data found intact on disk (a
resume or a verify), while `speed_bps` and `eta_seconds` count network bytes
only, averaged over 5 seconds. Installs, backlog changes and QR logins end with
a `{"event":"result",...}` line on stdout.

### Errors

| `type`          | Exit code | Meaning                                                                               |
| --------------- | --------- | ------------------------------------------------------------------------------------- |
| `auth_required` | 77        | Not signed in, or a sensitive view needs a fresh sign-in                              |
| `rate_limited`  | 75        | Steam is throttling this client                                                       |
| `disk_full`     | 85        | No space or quota left, including `install`'s check before it starts                  |
| `offline`       | 86        | No network route or DNS, or Steam is unreachable and gaki is using its cached library |
| the rest        | 1         | `invalid_input`, `not_found`, `access_denied`, `network_timeout`, ...                 |

A stopped install (`install stop`) ends as `cancelled` (83).
`gaki-cli capabilities` lists every type, with its code and hint.

## The session daemon

The first command that needs Steam starts a session daemon, and later commands
reuse its connection instead of logging on again. It serves one config
directory, and exits after 30 minutes with nothing to do
(`daemon_idle_timeout`). Long installs run inside it, so they survive the
command that started them.

```bash
gaki-cli daemon list                  # running daemons
gaki-cli daemon stop <pid>
gaki-cli kill                         # kill every running gaki-cli process, the daemon included
```

| Flag              | Effect                                    |
| ----------------- | ----------------------------------------- |
| `--no-daemon`     | Run the command in this process           |
| `--no-spawn`      | Use a running daemon, but never start one |
| `--socket <PATH>` | Talk to the daemon at this socket         |

[CONFIG.md](CONFIG.md#the-session-daemon) has the socket's location and its
security.

## Logs

Logs go to stderr. By default only warnings and errors show. `-v` adds
progress and debug logs and a launched game's own output, and `-vv`/`-vvv`
unmute the Steam networking stack, which helps when a command seems stuck.
Commands run through the daemon show the same logs at your `-v` level, and a
verbose `play` also shows the daemon's own activity, prefixed `[daemon]`.
`--log <FILTER>` (e.g. `--log debug,gaki_steam_vent=info`) overrides the level.

Each launch's output is saved under `~/.local/state/gaki/logs/`; `gaki-cli
clean --logs` deletes all but each game's newest.
