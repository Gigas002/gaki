# gaki: a command-line Steam client for Linux

**A fast, lightweight, headless Steam launcher and library manager written in Rust. Install, update and play your Steam games entirely from the terminal.**

> [!CAUTION]
> **gaki is experimental, and we don't recommend using it yet.** Commands,
> config files and behaviour still change without notice, and like every
> unofficial Steam client it sits in a gray zone of Steam's rules
> ([Steam rules and risks](#steam-rules-and-risks)). Try it on a spare
> account, not the one your library lives on.

gaki is a command-line Steam client and launcher for Linux. No CEF, no
WebViews, no GUI. It talks to Steam's real network protocols through
[`steam-vent`](https://codeberg.org/steam-vent/steam-vent), so you can log in,
manage your library, install and update games, sync Steam Cloud saves, manage
Workshop content, chat with friends, and launch titles natively or through
Proton, from a terminal or a script. It works alongside the official Steam
client, which must be installed.

Because it never needs a desktop session, it runs where the Steam client can't:
over SSH, on a headless box, in a container, or as one step in a shell script.
Every command speaks `--json`, so it also works as a backend for another
launcher. `gaki-tui` puts a terminal UI over the same library.

```bash
gaki-cli login
gaki-cli list --installed
gaki-cli install 1245620
gaki-cli play 1245620
```

---

## Why gaki

gaki isn't just Steam in a terminal. It does a few things the official
client doesn't:

- **Profiles.** A [profile](docs/CONFIG.md#profiles) is one file holding a
  runner (Proton or native), a prefix, environment variables, DLL overrides
  and wrappers like `mangohud` or `gamemoderun`. Any number of games can share
  one profile and its prefix, and a game's
  [`games.toml`](docs/CONFIG.md#gamestoml) entry only names what it changes.
  So "run these ten games with this Proton build and these tweaks" is one
  file, not ten trips through properties dialogs. Profiles use the same format
  as [lootrs](https://github.com/Gigas002/lootrs), so its Proton and native
  profiles work in gaki unchanged.
- **Scriptable from end to end.** Every command speaks `--json`, errors come
  with stable types and exit codes, and `gaki-cli capabilities` describes the
  build to whatever drives it. A session daemon keeps one login warm, so
  scripts don't sign in on every call ([GAKI-CLI.md](docs/GAKI-CLI.md#scripting-with---json)).
- **Your versions, your choice.** Install an older manifest and pin it so
  updates leave it alone, switch branches, or browse a depot's files and pull
  out a single one, with SteamDB links for the manifest history
  ([versions](docs/GAKI-CLI.md#versions-and-branches),
  [depot browser](docs/GAKI-CLI.md#depot-browser-depot-feature)).
- **Move your library.** `library export` writes the installed games to a
  TOML file, and `library import` installs them on another machine, one at a
  time through the resumable backlog
  ([how](docs/GAKI-CLI.md#moving-a-library-to-another-machine)).
- **Collections offline.** Edit collections without a connection and sync
  them with Steam when you choose ([collections](docs/GAKI-CLI.md#collections)).
- **Build only what you need.** Each capability is a Cargo feature, from
  "launch what's installed" up to the Store and the Market
  ([FEATURES.md](docs/FEATURES.md)).

---

## What it does

- **Library:** owned and Family-Shared games, local installs, search and
  filters, collections synced with Steam, achievements, DLC
  ([library commands](docs/GAKI-CLI.md#library)).
- **Installs:** parallel downloads from the least-loaded content servers,
  updates, verification, moves between libraries, and an install backlog that
  pauses, resumes and survives restarts. Downgrade to an older manifest and pin
  it there ([install commands](docs/GAKI-CLI.md#install-and-maintain)).
- **Launching:** native Linux builds, or Windows builds through Proton.
  [Profiles](docs/CONFIG.md#profiles) hold the runner, a shared prefix, the
  environment and wrappers, and games point at a profile. Steam integration
  (`auto`, `host`, `none`) bridges to the host Steam client for Steamworks and
  DRM ([launching](docs/GAKI-CLI.md#launch)).
- **Steam Cloud** save sync, and the **Workshop**: browse, subscribe,
  install, rate, comment ([Cloud](docs/GAKI-CLI.md#steam-cloud),
  [Workshop](docs/GAKI-CLI.md#steam-workshop)).
- **Friends and chat**, the **Store** (search, prices, deals, news, reviews,
  wishlists), and a read-only **inventory and Community Market**. gaki
  never moves real money or items: no buying, selling, trading or store
  purchases, now or later ([friends](docs/GAKI-CLI.md#friends-and-chat),
  [store](docs/GAKI-CLI.md#store),
  [inventory and market](docs/GAKI-CLI.md#inventory-and-market)).
- **A terminal UI:** `gaki-tui` covers the same library with keys, overlays
  and background loading ([GAKI-TUI.md](docs/GAKI-TUI.md)).
- **Slim builds:** every capability is a Cargo feature
  ([FEATURES.md](docs/FEATURES.md)).

gaki is in **experimental beta**; see the caution above.
[FAQ.md](docs/FAQ.md) answers common questions, and
[AUDIT.md](docs/AUDIT.md) records the security, performance and feature audit.

---

## Steam rules and risks

gaki was checked against Steam's rules on 2026-11-08: the Subscriber
Agreement, the Online Conduct rules, the Web API terms and the brand
guidelines. That was a technical review, not legal advice.

- **No rule is broken outright.** gaki signs in with Valve's own login,
  never stores your password, downloads only games you own, never sets
  achievements or fakes playtime, and **never buys, sells or trades**: the
  market, inventory and wallet are read-only.
- **But it is unofficial.** The Subscriber Agreement forbids
  "unauthorized third-party software" that interacts with Steam, which
  covers every unofficial client, and Valve may restrict an account without
  notice. How likely that is can't be read from the rules.
- **Scripting has limits.** Bots that chat, poll the market or farm
  playtime are against the rules, whatever runs them, and so is using a
  proxy to hide your country.

The full review, with every feature rated and the clauses quoted, is in
[docs/STEAM-RULES.md](docs/STEAM-RULES.md).

---

## Documentation

| Document                                     | Contents                                                                           |
| -------------------------------------------- | ---------------------------------------------------------------------------------- |
| [docs/GAKI-CLI.md](docs/GAKI-CLI.md)         | Every `gaki-cli` command, `--json` scripting, the session daemon, logs             |
| [docs/GAKI-TUI.md](docs/GAKI-TUI.md)         | The terminal UI: signing in, keys, the Actions menu, its settings                  |
| [docs/CONFIG.md](docs/CONFIG.md)             | `config.toml`, profiles, `games.toml`, where files live, the session               |
| [docs/FEATURES.md](docs/FEATURES.md)         | The Cargo features and what each one builds                                        |
| [docs/FAQ.md](docs/FAQ.md)                   | Prerequisites and common questions                                                 |
| [docs/STEAM-RULES.md](docs/STEAM-RULES.md)   | Steam's rules and what gaki does about them; the risks to an account               |
| [docs/AUDIT.md](docs/AUDIT.md)               | Security, performance and feature audit, with a matrix against the official client |
| [examples/](examples)                        | Commented example configuration                                                    |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Repository layout and the rules the code follows                                   |
| [docs/PLAN.md](docs/PLAN.md)                 | The fork plan: what changed from Aurelia, step by step                             |

---

## Crates

| Crate            | What it is                                                                       |
| ---------------- | -------------------------------------------------------------------------------- |
| `libgaki`        | All the logic: Steam session, library, installs, cloud, launch, config           |
| `gaki-steam-cdn` | The depot download engine `libgaki` installs with: manifests, depot keys, chunks |
| `gaki-cli`       | The `gaki-cli` command, a thin layer over `libgaki`                              |
| `gaki-tui`       | The `gaki-tui` terminal UI, a thin layer over `libgaki`                          |

None is on crates.io yet, and `libgaki`'s API may change before 1.0.
`gaki-steam-cdn` is gaki's own copy of
[`aurelia-steam-cdn`](https://github.com/Drackrath/aurelia-steam-cdn), Aurelia's
fork of [steam-cdn](https://github.com/OccamyLab/steam-cdn); its
[README](gaki-steam-cdn/README.md) lists what changed. The Steam protocol and
friend chat come from
[`aurelia-steam-vent`](https://crates.io/crates/aurelia-steam-vent) and
[`aurelia-steam-vent-chat`](https://crates.io/crates/aurelia-steam-vent-chat),
forks of [steam-vent](https://codeberg.org/steam-vent/steam-vent) and
[steam-vent-chat](https://codeberg.org/steam-vent/chat) published by Aurelia's
author.

---

## Acknowledgments

gaki is a fork of **[Aurelia](https://github.com/Drackrath/Aurelia)** and
**[Aurelia-TUI](https://github.com/Drackrath/Aurelia-TUI)**. Aurelia grew directly out of
**[SteamFlow](https://github.com/weter11/SteamFlow)**, and Aurelia-TUI out of
**[steam-tui](https://github.com/dmadisetti/steam-tui)**. Our deepest thanks to their
authors: they did the groundwork everything here is built on.

It stands, in turn, on **[steam-vent](https://codeberg.org/steam-vent/steam-vent)** and
**[steam-vent-chat](https://codeberg.org/steam-vent/chat)**, whose reverse-engineering and
protocol work let gaki speak Steam's real network protocols, and on
[`steam-cdn`](https://github.com/OccamyLab/steam-cdn), whose modified copy is the content
pipeline in `gaki-steam-cdn`.

### Credits

- [Aurelia](https://github.com/Drackrath/Aurelia) and [Aurelia-TUI](https://github.com/Drackrath/Aurelia-TUI): the CLI and TUI gaki is forked from
- [SteamFlow](https://github.com/weter11/SteamFlow): the project Aurelia is derived from. Its groundwork is the base everything here is built on. Thank you!
- [steam-tui](https://github.com/dmadisetti/steam-tui): the original TUI that Aurelia-TUI rebuilt
- [steam-vent](https://codeberg.org/steam-vent/steam-vent): Steam network protocol implementation (forked & modified as [aurelia-steam-vent](https://codeberg.org/Drackrath/aurelia-steam-vent))
- [steam-vent-chat](https://codeberg.org/steam-vent/chat): Steam Chat protocol implementation (forked & modified as [aurelia-steam-vent-chat](https://codeberg.org/Drackrath/aurelia-steam-vent-chat))
- [steam-cdn](https://github.com/OccamyLab/steam-cdn): content-delivery and depot download engine (forked & modified as [aurelia-steam-cdn](https://github.com/Drackrath/aurelia-steam-cdn), and from there as `gaki-steam-cdn`)
- [SteamKit2](https://github.com/SteamRE/SteamKit): Steam .NET research code
- [SteamHelper-rs](https://github.com/saskenuba/SteamHelper-rs): SteamKit Rust port

---
