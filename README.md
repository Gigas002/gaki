# gaki: a command-line Steam client for Linux

**A fast, lightweight, headless Steam launcher and library manager written in Rust. Install, update and play your Steam games entirely from the terminal.**

[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Status: Beta](https://img.shields.io/badge/status-experimental%20beta-yellow.svg)](#what-it-does)

<br clear="left" />

> [!WARNING]
> **Disclaimer: read before use.**
> gaki is an **independent, unofficial project** and is **not affiliated with, authorized, sponsored, or endorsed by Valve or Steam** in any way. "Steam" and "Valve" are trademarks of Valve Corporation.
>
> - **It modifies Steam's files directly.** Doing so may corrupt or damage your Steam installation, potentially forcing a full reinstallation. Back up your data first.
> - **No warranty for games launched outside the official Steam launcher.** Titles started through gaki bypass the normal Steam client and may not behave as expected.
>
> Use gaki entirely **at your own risk**. The authors accept no liability for damage to your Steam installation, lost data, or banned or suspended accounts.

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

## What it does

- **Library:** owned and Family-Shared games, local installs, search and
  filters, collections synced with Steam, achievements, DLC.
- **Installs:** parallel downloads from the least-loaded content servers,
  updates, verification, moves between libraries, and an install backlog that
  pauses, resumes and survives restarts. Downgrade to an older manifest and pin
  it there.
- **Launching:** native Linux builds, or Windows builds through Proton.
  [Profiles](docs/CONFIG.md#profiles) hold the runner, a shared prefix, the
  environment and wrappers, and games point at a profile. Steam integration
  (`auto`, `host`, `none`) bridges to the host Steam client for Steamworks and
  DRM.
- **Steam Cloud** save sync, and the **Workshop**: browse, subscribe,
  install, rate, comment.
- **Friends and chat**, the **Store** (search, prices, deals, news, reviews,
  wishlists), and a read-only **inventory and Community Market**.
- **Slim builds:** every capability is a Cargo feature
  ([FEATURES.md](docs/FEATURES.md)).

gaki is in **experimental beta**.

---

## Install

gaki isn't packaged or published yet. Build it from source with a
[Rust toolchain](https://rustup.rs/) (edition 2024). The keyring needs D-Bus
headers (Ubuntu: `sudo apt-get install pkg-config libdbus-1-dev`).

```bash
git clone https://github.com/Gigas002/gaki.git
cd gaki
cargo build --release
```

That builds `target/release/gaki-cli` and `target/release/gaki-tui` with the
default features. [FEATURES.md](docs/FEATURES.md) shows how to build more, or
less.

---

## Documentation

| Document | Contents |
| --- | --- |
| [docs/GAKI-CLI.md](docs/GAKI-CLI.md) | Every `gaki-cli` command, `--json` scripting, the session daemon, logs |
| [docs/GAKI-TUI.md](docs/GAKI-TUI.md) | The terminal UI: signing in, keys, the Actions menu, its settings |
| [docs/CONFIG.md](docs/CONFIG.md) | `config.toml`, profiles, `games.toml`, where files live, the session |
| [docs/FEATURES.md](docs/FEATURES.md) | The Cargo features and what each one builds |
| [docs/FAQ.md](docs/FAQ.md) | Prerequisites and common questions |
| [examples/](examples) | Commented example configuration |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Repository layout and the rules the code follows |
| [docs/PLAN.md](docs/PLAN.md) | The fork plan: what changed from Aurelia, step by step |

---

## Crates

| Crate | What it is |
| --- | --- |
| `libgaki` | All the logic: Steam session, library, installs, cloud, launch, config |
| `gaki-cli` | The `gaki-cli` command, a thin layer over `libgaki` |
| `gaki-tui` | The `gaki-tui` terminal UI, a thin layer over `libgaki` |

None is on crates.io yet, and `libgaki`'s API may change before 1.0. Two
dependencies are forks published by Aurelia's author:
[`aurelia-steam-cdn`](https://crates.io/crates/aurelia-steam-cdn) (depot
downloads, from [steam-cdn](https://github.com/OccamyLab/steam-cdn)) and
[`aurelia-steam-vent-chat`](https://crates.io/crates/aurelia-steam-vent-chat)
(friend chat, from [steam-vent-chat](https://codeberg.org/steam-vent/chat)).

---

## Acknowledgments

gaki is a fork of **[Aurelia](https://github.com/Drackrath/Aurelia)** and
**[Aurelia-TUI](https://github.com/Drackrath/Aurelia-TUI)**. Aurelia grew directly out of
**[SteamFlow](https://github.com/weter11/SteamFlow)**, and Aurelia-TUI out of
**[steam-tui](https://github.com/dmadisetti/steam-tui)**. Our deepest thanks to their
authors: they did the groundwork everything here is built on.

It stands, in turn, on **[steam-vent](https://codeberg.org/steam-vent/steam-vent)** and
**[steam-vent-chat](https://codeberg.org/steam-vent/chat)**, whose reverse-engineering and
protocol work let gaki speak Steam's real network protocols, and on a modified fork of
[`steam-cdn`](https://github.com/OccamyLab/steam-cdn) (plus the `zip` crate) for the content
pipeline.

### Credits

- [Aurelia](https://github.com/Drackrath/Aurelia) and [Aurelia-TUI](https://github.com/Drackrath/Aurelia-TUI): the CLI and TUI gaki is forked from
- [SteamFlow](https://github.com/weter11/SteamFlow): the project Aurelia is derived from. Its groundwork is the base everything here is built on. Thank you!
- [steam-tui](https://github.com/dmadisetti/steam-tui): the original TUI that Aurelia-TUI rebuilt
- [steam-vent](https://codeberg.org/steam-vent/steam-vent): Steam network protocol implementation
- [steam-vent-chat](https://codeberg.org/steam-vent/chat): Steam Chat protocol implementation (forked & modified as [aurelia-steam-vent-chat](https://codeberg.org/Drackrath/aurelia-steam-vent-chat))
- [steam-cdn](https://github.com/OccamyLab/steam-cdn): content-delivery and depot download engine (forked & modified as [aurelia-steam-cdn](https://github.com/Drackrath/aurelia-steam-cdn))
- [SteamKit2](https://github.com/SteamRE/SteamKit): Steam .NET research code
- [SteamHelper-rs](https://github.com/saskenuba/SteamHelper-rs): SteamKit Rust port

---

## License

gaki is released under the [GPL-3.0-or-later License](LICENSE). The `gaki-tui` crate,
imported from Aurelia-TUI, is GPL-3.0-only.

gaki is a fork of [Aurelia](https://github.com/Drackrath/Aurelia), whose early history
(up to commit `065782bc`) was MIT-licensed, and which was derived from
[SteamFlow](https://github.com/weter11/SteamFlow) up to its commit `c0222b75`. That code
is still in the tree, so its MIT notice is kept here:

```text
MIT License

Copyright (c) 2026 Drackrath

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
