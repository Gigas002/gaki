# Cargo features

Every capability of gaki is a Cargo feature defined in `libgaki`. The two
binaries forward the same names (`library = ["authentication",
"libgaki/library"]`) and gate their commands, views, key bindings and help text
on them, so a slim build has nothing it can't run and offers nothing it doesn't
have.

## The features

| Feature          | Default | Requires         | `libgaki`                                                                    |
| ---------------- | :-----: | ---------------- | ---------------------------------------------------------------------------- |
| *(core)*         | always  | —                | config and profiles, local library scan (ACF), Proton discovery, launch-pipeline stages and runners |
| `authentication` | on      | —                | login (password, Steam Guard, QR), session store and crypto, keyring, the CM connection |
| `library`        | on      | `authentication` | owned apps, appinfo/PICS, tags, achievements, licenses, DLC toggles, launching |
| `install`        | on      | `library`        | install, update, verify, uninstall, move/relink/import, backlog, official Proton downloads |
| `cloud`          | on      | `library`        | Steam Cloud save sync                                                        |
| `collections`    | on      | `library`        | library collections (CloudConfigStore)                                       |
| `versioning`     | off     | `install`        | branches, set-branch, downgrade, pin/unpin, manifests                        |
| `depot`          | off     | `install`        | depot browser: depot listings, manifest file lists, single-file downloads    |
| `workshop`       | off     | `install`        | Workshop list, browse, subscribe, rate, comments                             |
| `social`         | off     | `authentication` | friends, chat, presence, persona lookups by name                             |
| `inventory`      | off     | `authentication` | inventory, wallet                                                            |
| `market`         | off     | `inventory`      | Community Market listings, search, prices                                    |
| `store`          | off     | `library`        | search, price, deals, news, reviews, events, wishlist, similar, players      |

A feature turns on what it requires (`install` brings `library`, which brings
`authentication`), so no combination fails to build. Launching needs
`library`: it reads launch options from PICS.

### Optional dependencies

Each heavy dependency is `optional = true` and comes only with the feature that
uses it.

| Feature          | Brings in                                                                        |
| ---------------- | -------------------------------------------------------------------------------- |
| `authentication` | `steam-vent` (+ protos), `protobuf` (+ codegen), `reqwest`, `keyring-core` + `dbus-secret-service-keyring-store`, `secrecy`, `steam-vdf-parser`, `keyvalues-serde`, `tokio-stream`; in `gaki-cli` also `qrcode`, `rpassword` |
| `install`        | `steam-cdn`                                                                      |
| `cloud`          | `zip`, `filetime`, `sha1`                                                        |
| `depot`          | `hex`, `sha1`                                                                    |
| `social`         | `steam-vent-chat`, `steamid-ng`; in `gaki-cli` also `tokio-stream`               |

`build.rs` generates a protobuf only for the feature that uses it:
`collections` gets CloudConfigStore, and `store` gets StoreQuery and Wishlist.

## `gaki-cli`

`gaki-cli` forwards all twelve features with the same defaults.

| Feature          | Commands                                                                         |
| ---------------- | -------------------------------------------------------------------------------- |
| *(always)*       | `capabilities`, `config`, `profiles`, `proton list`, `proton default`            |
| `authentication` | `login`, `logout`, `daemon`, `kill`                                              |
| `library`        | `list`, `info`, `account`, `play`, `launch-targets`, `stop`, `running`, `drm`, `dlc`, `enable`, `disable`, `achievements`, `image`, `depots`, `launch-options`, `libraries` (plus the hidden `appinfo`, `tags`) |
| `install`        | `install`, `update`, `uninstall`, `verify`, `available`, `move`, `relink`, `import`, `duplicates`, `proton install`, `proton uninstall` |
| `versioning`     | `branches`, `set-branch`, `manifests`, `downgrade`, `pin`, `unpin`               |
| `cloud`          | `cloud`                                                                          |
| `collections`    | `collections`                                                                    |
| `workshop`       | `workshop`                                                                       |
| `social`         | `friends`, `chat`, `user`                                                        |
| `inventory`      | `inventory`, `wallet`                                                            |
| `market`         | `market`                                                                         |
| `store`          | `price`, `search`, `deals`, `players`, `events`, `news`, `reviews`, `wishlist`, `similar` |
| `depot`          | none yet (only the library API)                                                  |

The session daemon needs `authentication`, because without a login there is no
session to keep. `--no-daemon`, `--no-spawn` and `--socket` are still accepted in
a build without it, but hidden, so scripts can always pass them.

Without `social`, a user can be named only by SteamID or `me`; resolving a name
matches it against the friends list. `gaki-cli capabilities` lists the build's
features under `build_features`, and only the commands it has under `commands`.

## `gaki-tui`

A TUI without a library has nothing to show, so `gaki-tui` always builds
`authentication` and `library` and forwards the other ten. Its defaults are
`install`, `cloud` and `collections`.

| Feature       | Views and keys                                                                     |
| ------------- | ---------------------------------------------------------------------------------- |
| *(always)*    | sign-in pages, library list and tabs 1–4, detail and art, Actions menu, achievements `a`, DLC `D`, depots `o`, launch options `L`, launch targets, game settings, Proton `P` (set the default), running `R`, account `A`, settings `p`, favourite `f`, hide `H`, refresh `r`, ProtonDB page `B`, Proton issues `T` |
| `install`     | install `d` (with the library picker), pause/resume `Space`, cancel `c`, uninstall `x`, move `M`, relink `K`, import `N`, verify `v`, update `U`, Proton install/uninstall (`P` then `i`/`u`), the "install now?" prompt |
| `versioning`  | versions and pinning `V`, beta branches `b`                                        |
| `cloud`       | cloud saves `C`                                                                    |
| `collections` | collections `O`                                                                    |
| `workshop`    | Workshop `W`                                                                       |
| `social`      | the Friends tab (`5`, `F`, the Tab ring), chat, add/remove friend, `--chat` window |
| `inventory`   | inventory `I`, wallet `w`                                                          |
| `market`      | market listings `m`, market search `S`                                             |
| `store`       | none yet                                                                           |
| `depot`       | none yet                                                                           |

The help overlay, the status-bar hints and the Proton overlay's title list only
the keys the build has. If the build lacks `install`, Play on a game that isn't
installed says so instead of offering to install it.

## Building a slim binary

```sh
# Only local management: config, profiles, Proton discovery.
cargo build -p gaki-cli --release --no-default-features

# Log in and launch what's installed; no downloads.
cargo build -p gaki-cli --release --no-default-features --features library

# The defaults plus the Store and the Community Market.
cargo build -p gaki-cli --release --features store,market

# A library-only TUI.
cargo build -p gaki-tui --release --no-default-features
```

## The three-level matrix

CI runs build, clippy, tests and docs at three levels (see `ARCHITECTURE.md`
§1.4 and §8):

| Level   | Flag                    | What it validates                               |
| ------- | ----------------------- | ----------------------------------------------- |
| Minimal | `--no-default-features` | the core builds with no optional surface        |
| Default | *(none)*                | what a user installs                            |
| Full    | `--all-features`        | every optional integration builds and tests together |

The three levels don't catch a feature that only builds alongside another one.
Before merging a change to the gates, also check each feature on its own:

```sh
for f in authentication library install cloud collections versioning \
         depot workshop social inventory market store; do
  cargo clippy -p libgaki -p gaki-cli --all-targets \
    --no-default-features --features "$f" -- -D warnings || echo "FAILED: $f"
done
for f in install cloud collections versioning depot workshop social \
         inventory market store; do
  cargo clippy -p gaki-tui --all-targets \
    --no-default-features --features "$f" -- -D warnings || echo "FAILED: $f"
done
```

## Adding or gating a feature

- Define it in `libgaki/Cargo.toml`, with what it requires and the
  `dep:` dependencies only it needs. Forward it from `gaki-cli` and, unless
  it's `authentication` or `library`, from `gaki-tui`.
- Gate at module boundaries (`#[cfg(feature = "...")]` on `mod`) where the
  module is all one feature; otherwise gate the items, and the match arms and
  enum variants that name them.
- A rustdoc link to an item that another build gates out breaks
  `cargo doc -D warnings` there. Write the name as plain code instead.
- Gate a test with what it exercises, so every level runs the tests it can.
- Add the commands, views and keys to the tables above.
