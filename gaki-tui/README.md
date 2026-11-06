# gaki-tui

A terminal UI for browsing, installing, updating and launching your Steam
library on Linux. It's part of [gaki](../README.md) and runs everything
in-process through `libgaki`, the same library behind `gaki-cli`.

```bash
cargo build --release -p gaki-tui
./target/release/gaki-tui
```

[docs/GAKI-TUI.md](../docs/GAKI-TUI.md) covers signing in, the keys, the
Actions menu and its settings. [docs/CONFIG.md](../docs/CONFIG.md) covers the
configuration it shares with `gaki-cli`.

## Credits

- [steam-tui](https://github.com/dmadisetti/steam-tui): the original client
  this is rebuilt from, by [@dmadisetti](https://github.com/dmadisetti) and
  contributors
- [Aurelia-TUI](https://github.com/Drackrath/Aurelia-TUI): the TUI gaki-tui is
  forked from, built on [Aurelia](https://github.com/Drackrath/Aurelia)
- [steam-vent](https://codeberg.org/steam-vent/steam-vent): the Steam protocol
  implementation

## License

Distributed under GPL-3.0-only. See [`LICENSE`](../LICENSE).
