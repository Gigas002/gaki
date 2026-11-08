# FAQ

## What do I need before I start?

- **Linux.** gaki runs on Linux only.
- **The official Steam client, installed.** gaki works alongside it rather than
  replacing it: games that need Steamworks or DRM bridge to it, and official
  Proton builds install through it. You don't have to keep it open, but it has
  to be there.
- **Proton, if you play Windows games.** gaki uses an official Proton build
  that Steam installed (`gaki-cli proton install` asks Steam for one), or a
  build installed system-wide by your package manager, such as GE-Proton or
  proton-cachyos, named in a profile's `proton.build`. gaki doesn't download
  third-party builds itself.
- **A keyring** (Secret Service) for the session's tokens. Without one, set
  `secret_store = "memory"` ([CONFIG.md](CONFIG.md#session-and-security)).

## Is there a Steam client that works without a GUI?

That's what gaki is. You can log in, install and update games, sync Cloud
saves and launch titles from a terminal or a script. No Electron, no CEF, no
embedded browser.

## Can I install and play games over SSH or on a headless server?

Library management, downloads, updates, verification, Cloud sync and the
Workshop all work over SSH. Launching a game still needs somewhere for it to
render (a display, a virtual X server, or a remote-play setup), but everything
before the launch doesn't.

## How is gaki different from SteamCMD?

[SteamCMD](https://developer.valvesoftware.com/wiki/SteamCMD) is Valve's
official command-line tool, and it is content-only: it downloads and updates
app and Workshop files. gaki is a launcher and library manager too. It lists
and searches your library, launches games natively or through Proton, syncs
Steam Cloud saves, manages DLC and Workshop subscriptions, reads achievements,
and does friends and chat. SteamCMD is proprietary; gaki is open source under
GPL-3.0-or-later.

## Does it run Windows games?

Yes, through Proton. Point a profile at an official build or a system-wide one,
share its prefix between as many games as you like, and pick a profile per
game ([CONFIG.md](CONFIG.md#profiles)).

## Can I script my Steam library?

Every command takes `--json` for machine-readable output, errors included, and
`gaki-cli capabilities` lets a driver discover what a build supports
([GAKI-CLI.md](GAKI-CLI.md#scripting-with---json)). `--config-dir` keeps an
embedding driver's state apart from a standalone install.

## Can I build a smaller gaki?

Yes. Every capability is a Cargo feature, so a build can leave out the store,
the market, friends and chat, or even installing
([FEATURES.md](FEATURES.md)).

## Is it safe? Can I get VAC banned?

gaki is unofficial and unaffiliated with Valve, it edits Steam's files
directly, and third-party tools that talk to Steam carry a risk of action
against your account. Read the disclaimer at the top of the
[README](../README.md) first, and use gaki at your own risk.

VAC looks for cheats in VAC-secured games. gaki has no cheat features and
doesn't modify a game while it runs; it only starts it. The wider question is Steam's rules on unofficial
clients: [STEAM-RULES.md](STEAM-RULES.md) goes through them, says what gaki
does about each, and what you shouldn't script with it.
