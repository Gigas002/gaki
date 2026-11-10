# Audit

The final Phase 8 audit (PLAN step 8.14), in three parts: security (8.14a),
performance (8.14b) and supported features (8.14c). Each finding says what
was found, how serious it is, and what was done: **fixed** in the audit
step, **accepted** with the reason, or moved to a **follow-up** PLAN step.

Severity: **high** (an attacker gains code execution, credentials or writes
outside gaki's directories), **medium** (a crash, a hang or an unbounded
allocation from outside input; unsound `unsafe`), **low** (hardening; needs
an unlikely setup or an attacker who already controls something trusted).

## 1. Security (8.14a, 2026-11-08)

Reviewed by reading the code: every `unsafe` block, every place outside
data becomes a path, a process or an allocation, and how secrets are stored,
sent and logged. `cargo deny check` passes.

### 1.1 Secrets and the session

| Area                   | Finding                                                                                                                                                                                                   | Status         |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- |
| Refresh and web tokens | Only in the OS keyring (Secret Service) or, with `secret_store = "memory"` / `GAKI_SECRET_STORE=memory`, in process memory. `account.toml` names the account and which secrets exist, never their values. | OK             |
| Web cookie             | `WebSession` keeps `steamLoginSecure` in a `SecretString`: redacted in `Debug`, zeroed on drop.                                                                                                           | OK             |
| Logs                   | No `tracing` or console line prints a token, password, cookie or refresh token; errors are logged by their message only. The daemon logs no request frames.                                               | OK             |
| Logout                 | `logout` revokes the refresh token at Steam and reports when it couldn't.                                                                                                                                 | OK             |
| Memory store           | `Memory` holds secrets as plain `String`s, so they aren't zeroed on drop. It's opt-in and lives as long as the process anyway.                                                                            | Accepted (low) |

### 1.2 Daemon socket

`$XDG_RUNTIME_DIR/gaki/daemon.sock`: the directory is created `0700` (or
tightened, and refused if another user owns it), the socket is `0600`, every
connection's peer uid is checked with `SO_PEERCRED`, and there is no `/tmp`
fallback. Frames are length-prefixed and capped at 64 MiB. A `--socket`
override is still `0600` and peer-checked. OK.

### 1.3 Untrusted input

| Area                         | Finding                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              | Severity | Status                                    |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------- | ----------------------------------------- |
| Depot chunks                 | A downloaded chunk was written without checking it against the manifest: its SHA-1 and size were only compared when re-verifying files already on disk. A corrupt or substituted chunk (or one longer than its slot, overwriting the next) went to disk as is. `get_chunk` now takes the manifest's `ChunkData` and refuses a chunk whose size or SHA-1 differs; the refusal is retried like a corrupt transfer.                                                                                                                     | medium   | Fixed                                     |
| Chunk decompression          | The VZ (LZMA) decoder subtracted header sizes from the data length before checking it (an underflow on short data) and pre-allocated the size its footer claims (up to 4 GiB). The VSZ (Zstandard) and ZIP paths read without limit. All three now stop at the manifest's size for the chunk, and VZ checks its length first. Chunks are encrypted with the depot key, so forging one needs that key.                                                                                                                                | medium   | Fixed                                     |
| Manifest ZIP                 | A zipped manifest was unzipped without limit (a decompression bomb from a CDN server, or over plain HTTP). Now capped at 512 MiB; real manifests are tens of MB.                                                                                                                                                                                                                                                                                                                                                                     | medium   | Fixed                                     |
| Cloud ZIP                    | A Cloud download that Steam stored zipped was unzipped without limit, and its buffer pre-allocated from a size field Steam controls. Now capped at 1 GiB, and the size field is only a bounded hint.                                                                                                                                                                                                                                                                                                                                 | medium   | Fixed                                     |
| Manifest paths               | Every entry's path and symlink target is checked before anything is created (`gaki-steam-cdn::cdn::paths`): no absolute paths, no `..` out of the install directory, no writing through an existing symlink, links pointing outside are skipped. Windows `\` separators are normalised before the check.                                                                                                                                                                                                                             | —        | OK                                        |
| Manifest signatures          | Manifests aren't signature-checked (the key and scheme aren't known yet). With chunks now hash-checked, a forged manifest can still rearrange or drop files of the depot, not inject new content without the depot key.                                                                                                                                                                                                                                                                                                              | low      | Accepted (8.16: key not public, see §1.5) |
| CDN transport                | HTTPS whenever any content server offers it; plain HTTP only when none does.                                                                                                                                                                                                                                                                                                                                                                                                                                                         | —        | OK                                        |
| Cloud file names             | `%Root%` tokens map to fixed directories, and the rest is joined with empty, `.` and `..` components dropped, so a crafted name can't leave its root.                                                                                                                                                                                                                                                                                                                                                                                | —        | OK                                        |
| `installdir`                 | Every PICS or `appmanifest` install directory is checked to be one plain name before use, except in `import_install`. It now checks too.                                                                                                                                                                                                                                                                                                                                                                                             | low      | Fixed                                     |
| Launch entries               | The executable and working directory of a PICS launch entry are joined to the install directory as given. Running a Valve launch entry trusts Valve as much as running the game's binary does.                                                                                                                                                                                                                                                                                                                                       | low      | Accepted                                  |
| protobuf (RUSTSEC-2024-0437) | Stack overflow on deeply nested messages. `steam-vent-proto-common` 0.5.1 (under `steam-vent`, also after the 8.21 switch to the Aurelia fork) pins `=3.5.1`. The `deny.toml` note said the input was only CM traffic; depot manifests from CDN servers are parsed with it too. The worst case is a crash. Note corrected. Fixed by vendoring steam-vent as `gaki-steam-vent` + `gaki-steam-vent-proto`: the bindings are regenerated in `build.rs` against the workspace `protobuf` (>= 3.7.2), and the `deny.toml` ignore is gone. | low      | Fixed                                     |
| rsa (RUSTSEC-2023-0071)      | Marvin timing attack on RSA decryption. `steam-vent-crypto` pinned `rsa` 0.9, which has no fix. gaki only ever encrypts the password with Steam's public key, so no private key was there to leak, and `deny.toml` ignored it. Fixed by vendoring the crypto as `gaki-steam-vent-crypto` on `rsa` 0.10; the ignore is gone.                                                                                                                                                                                                          | —        | Fixed                                     |
| LZMA decoder                 | `lzma-rs` was unmaintained (no release since 2023). VZ chunks are now decoded with `lzma-rust2` (maintained, pure Rust), which sizes its dictionary to the claimed output, so a bad header can't make it allocate more. The claimed size is checked against the chunk's size first, and the output length and CRC32 after.                                                                                                                                                                                                           | low      | Fixed (8.17)                              |
| Config and profile files     | The user's own files; parse errors become `Issue`s or load errors, never panics.                                                                                                                                                                                                                                                                                                                                                                                                                                                     | —        | OK                                        |
| Panics on input              | No `unwrap`/`expect` on outside data in the parsers; string slicing on Steam text stays on character boundaries; the chat wrapper guards a zero width.                                                                                                                                                                                                                                                                                                                                                                               | —        | OK                                        |

### 1.4 Processes and `unsafe`

| Area               | Finding                                                                                                                                                                                                                                                                                                                                                        | Severity | Status |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ------ |
| Launch diagnostics | `LaunchContext` carried a raw `*mut LaunchVerification` into the pipeline's own state, with hand-written `unsafe impl Send`/`Sync`. A `Clone`d context outliving the pipeline, or sent to another thread, would write through a dangling or shared pointer. Replaced by an `Arc<Mutex<LaunchVerification>>` the stages copy back; the `unsafe impl`s are gone. | medium   | Fixed  |
| Browser launchers  | `xdg-open` was handed whatever URL it got. All of gaki's URLs are built from ids except a user profile's page, which comes from Steam's profile data; a `file:` URL or a `.desktop` path could make `xdg-open` run something. The TUI opener and the OpenID helper now accept only `http(s)://` URLs.                                                          | low      | Fixed  |
| Other spawns       | `proton run`, wrappers and `env` come from the user's own config; `steam -silent` and the chat window run fixed programs with fixed or validated arguments (a friend's name is passed as `--name=…`).                                                                                                                                                          | —        | OK     |
| Remaining `unsafe` | `std::env::set_var` for the proxy (documented to run before any thread starts), `libc::getuid`, `libc::kill`, and `setsid` in a `pre_exec` (async-signal-safe).                                                                                                                                                                                                | —        | OK     |

### 1.5 Manifest signatures (8.16, 2026-11-08)

The step was to verify each depot manifest's signature, or record why that
can't be done. It can't, for now:

- **The format is known.** A manifest from the CDN has three sections:
  payload, metadata, then `ContentManifestSignature`, which holds one
  128-byte value (`signature`): an RSA-1024 signature. Checked on the
  manifest of depot 2835573 as the CDN served it.
- **The official client keeps no signatures.** All 15 manifests in its
  `depotcache` have an empty signature section, so they can't be used to
  work the scheme out.
- **Valve's public "system" key isn't the signer.** Applying it (the key
  steam-vent shipped as `system.pem` for the TCP channel's session key) to the
  signature gives neither PKCS#1 v1.5 nor PSS padding, read big-endian or
  little-endian. So the manifest key is a different one.
- **No open-source client knows it.** SteamKit2 and DepotDownloader parse
  the section and ignore it, and nothing public names the key or what the
  signature covers.

The one remaining source is the official client's own binary, which would
embed the key. Taking it from there means inspecting Valve's proprietary
code, which the Steam Subscriber Agreement restricts, so it wasn't done
(user decision, 2026-11-08).

Without the signature check, a download is still protected by HTTPS to the
content server, by the manifest request code that only an owner of the
depot can get, and by the SHA-1 and size check on every chunk (8.14a). A
forged manifest would also need the depot key to make chunks that decrypt
to anything. What it could still do is drop files or rearrange the depot's
own chunks. Severity stays **low**. Revisit if Valve or an open-source
client publishes the key.

## 2. Performance (8.14b, 2026-11-08)

Measured on the test account with release builds (`cargo build --release
--all-features`) on one desktop machine (NVMe disk, Steam cell in Tokyo).
Wall times are medians of five runs unless one run is given. Peak memory is
the process's peak RSS. The test game is Buckshot Roulette (app 2835570),
Linux depot: an 864 MiB download, 1.25 GiB on disk, 14 files. One of them
is an 811 MB executable. Network times vary from run to run with the
connection Manager Steam picks, so treat them as rough.

### 2.1 Startup

| What                                                   | Time                                                                                      | Memory          | Notes                                                                                                                                                                                                                    |
| ------------------------------------------------------ | ----------------------------------------------------------------------------------------- | --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `gaki-cli --help`                                      | 2.2 ms                                                                                    | 15.5 MiB        |                                                                                                                                                                                                                          |
| `capabilities --json`, `profiles list`, `config check` | 2.8–3.9 ms                                                                                | 15 MiB          | Offline.                                                                                                                                                                                                                 |
| `--no-daemon list --installed`                         | 48 ms                                                                                     | 27 MiB          | Local disk only.                                                                                                                                                                                                         |
| First forwarded command, daemon not running            | 3.9–5.5 s                                                                                 | —               | The daemon binds its socket at once, then restores the session. A traced run: CM list 0.2 s, websocket TLS handshake to the CM 1.3 s, logon reply 0.7 s. The same command in-process (`--no-daemon account`) took 2.9 s. |
| Forwarded commands, daemon running                     | `account` 170–350 ms, `dlc` 210 ms, `list` 400 ms, `achievements` 450 ms, `update` 640 ms | 15 MiB (client) | One or two Steam round trips each. `info` answers from the info cache in 3 ms.                                                                                                                                           |
| `gaki-tui` first frame                                 | 10–50 ms                                                                                  |                 | The library then lists in 4.3 s on a cold start (session restore plus `list`).                                                                                                                                           |

| Finding                                                                                                                                                                                                                                                                                                                         | Status |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| `config check` (and every `config` subcommand) was forwarded to the daemon, so an offline check started a daemon and a Steam logon it never used. `config` now runs in-process like `profiles`: it only reads and writes config files, and its messages already tell the user to restart the daemon for keys it reads at start. | Fixed  |

### 2.2 Install, verify, launch

| What                                                | gaki                                                                  | Official client                                                                                                                                                                |
| --------------------------------------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Fresh install of the Linux depot                    | 14.0–15.5 s, including a 1–2 s logon; peaks near 150 MiB/s            | 14 s from `steam://validate` to done, 12 s of it downloading (files deleted, appmanifest kept; it first installed two Steam Linux Runtime apps the game requires, not counted) |
| Memory during the install                           | 145–149 MiB peak for the whole process (276 MiB before the fix below) | The `steam` process grew from 333 to 461 MiB (+128 MiB)                                                                                                                        |
| `verify` (1.25 GiB)                                 | 2.9 s, about 430 MiB/s                                                | —                                                                                                                                                                              |
| `update` check                                      | 3.9 s cold, of which 1.2 s logon                                      | —                                                                                                                                                                              |
| Native launch: `play` until the game's process runs | 1.3 s                                                                 | —                                                                                                                                                                              |
| `stop` until `play` returns                         | 0.5 s, nothing left running                                           | —                                                                                                                                                                              |

gaki also verified the files the official client wrote without finding a
difference.

| Finding                                                                                                                                                                                                                                                                                                                                                                                              | Status |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| Chunks were collected in file order (`FuturesOrdered`), so every chunk that finished behind a slow one waited in memory until that one arrived. With 16 requests in flight and an 811 MB file this is most of the peak. Chunks are now written as they finish, each at its own offset (`FuturesUnordered`); peak memory fell from 276 to 149 MiB at the same speed, and `verify` confirms the files. | Fixed  |
| A CDN auth token was requested for every content server, but Steam only gives one for `CDN`-type servers; every `SteamCache` server answered with an error (logged as a warning), one round trip per host per depot. Tokens are now asked for only where the server type takes one. Installs still work with none requested.                                                                         | Fixed  |
| A native Linux launch resolved a Wine runner anyway: the DLL-provider stage looked for one and logged that `wine` couldn't be resolved, and the effective config reported a runner root. Both are now skipped when the launch target is `NativeLinux`.                                                                                                                                               | Fixed  |

### 2.3 Requests to Steam

One fresh install, counted from the debug log (CM messages):

| Request                                                                         | Before           | After |
| ------------------------------------------------------------------------------- | ---------------- | ----- |
| Logon (with `Hello`)                                                            | 1                | 1     |
| PICS product info for the same app                                              | 5                | 1     |
| Depot decryption key                                                            | 1                | 1     |
| Service calls (registering the install, content servers, manifest request code) | 4                | 4     |
| `GetCDNAuthToken` for `SteamCache` hosts                                        | 2+ (all refused) | 0     |

One install asked PICS for the same app's info five times: for its name, its
DLC parent, its depots, its build id and the appmanifest. PICS replies are
now reused for 10 seconds, per app id. That's long enough to cover a single
operation, and short enough that a long-lived daemon never serves an old build
id. That's well inside Steam's rate limits either way. Store and web calls
(`price`, `search`, `news`, `reviews`) are one request per command and
user-driven, and the TUI loads them only when an overlay opens.

### 2.4 TUI responsiveness

Driven through a pseudo-terminal: every overlay key tried (game info `n`,
depots `D`, branches `b`, user profile `V`, Workshop `W`, Store `Y`, installs
`X`) redraws within 1–11 ms. The overlays load in the background
(`browser/job`) and show a loading state meanwhile.

| Finding                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | Status           |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------- |
| Move, relink, import and uninstall ran on the UI thread. The TUI froze for the whole copy or delete, which for a large game is minutes. They now run as one background disk job. Its outcome goes to the prompt that started it, or to the status bar once that prompt is closed, and it refreshes that game's install state. Only one runs at a time; starting a second one says so. Uninstall used to reload the whole library afterwards; it now refreshes just that game. | Fixed            |
| Toggling a DLC still calls Steam on the UI thread: 0.3 s cold (one PICS request), instant while the PICS reply is reused. Switching branch, stopping a game, choosing the default Proton and logging out are local or one short call.                                                                                                                                                                                                                                         | Accepted (short) |

## 3. Supported features (8.14c, 2026-11-08)

What the official Steam client does, and what gaki has of it: `libgaki`
(the library), `gaki-cli` and `gaki-tui`. Checked against the code, the
help and the docs, not only the docs. **✓** means it's there, **part** that
some of it is (the note says which), **—** that it isn't. Every gap has a
reason:

- **non-goal**: one of the PLAN's non-goals (money and items with money
  value, localization, third-party runtimes, lootrs' `wine`/`umu`).
- **needs Steam**: the official client has to be running (R21). gaki
  launches with `play --steam` (or a profile's Steam integration), so the
  running client provides it.
- **not done**: possible with what gaki has, not built yet.
- **not possible**: no protocol a third-party client can use for it.

The pure GUI parts of the official client (Big Picture, the in-game
overlay's own UI, broadcasting, themes) aren't listed. A terminal client has
no counterpart to them.

### 3.1 Account

| Official client                                                         | libgaki |    gaki-cli    |    gaki-tui    | Gap                                                                                                                                |
| ----------------------------------------------------------------------- | :-----: | :------------: | :------------: | ---------------------------------------------------------------------------------------------------------------------------------- |
| Sign in with password and Steam Guard (email or mobile code)            |    ✓    |   ✓ `login`    | ✓ sign-in page |                                                                                                                                    |
| Sign in with a QR code                                                  |    ✓    | ✓ `login --qr` |     ✓ `y`      |                                                                                                                                    |
| Stay signed in; sign out (revoking the token)                           |    ✓    |   ✓ `logout`   |  ✓ `A` → `o`   |                                                                                                                                    |
| Account details                                                         |    ✓    |  ✓ `account`   |     ✓ `A`      |                                                                                                                                    |
| Offline mode                                                            |  part   |      part      |      part      | Automatic: when Steam can't be reached, the library is served from the cache. There's no switch to go offline on purpose. not done |
| Steam Guard authenticator, account security settings, family management |    —    |       —        |       —        | not done (gaki shows family-shared games in the library, but can't manage the family)                                              |

### 3.2 Library

| Official client                              |             libgaki              |     gaki-cli     |         gaki-tui          | Gap                                                                                                     |
| -------------------------------------------- | :------------------------------: | :--------------: | :-----------------------: | ------------------------------------------------------------------------------------------------------- |
| Owned and family-shared games                |                ✓                 |     ✓ `list`     |         ✓ library         |                                                                                                         |
| Store details, DLC, tags, requirements       |                ✓                 | ✓ `info`, `dlc`  |  ✓ detail pane, `n`, `D`  |                                                                                                         |
| Artwork                                      |                ✓                 |    ✓ `image`     | ✓ art pane, `n` → Artwork |                                                                                                         |
| Achievements (view)                          |                ✓                 | ✓ `achievements` |           ✓ `a`           |                                                                                                         |
| Playtime                                     |                ✓                 | ✓ `list --sort`  |    ✓ detail pane, `s`     | Total, last two weeks and last played; read only.                                                       |
| Collections, synced with Steam               |                ✓                 | ✓ `collections`  |           ✓ `O`           |                                                                                                         |
| Favourites and hidden games                  | ✓ (Steam's built-in collections) | ✓ `collections`  |             ✓             | The TUI's `f`/`H` and Favourites tab use Steam's built-in collections, synced in the background (8.18). |
| Add a non-Steam game, activate a product key |                —                 |        —         |             —             | not planned                                                                                             |
| Screenshots, recordings, notes               |                —                 |        —         |             —             | not planned                                                                                             |

### 3.3 Install and manage

| Official client                                                   | libgaki |               gaki-cli               |       gaki-tui       | Gap                                                                                                                                                                                                                          |
| ----------------------------------------------------------------- | :-----: | :----------------------------------: | :------------------: | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Install, update, verify, uninstall                                |    ✓    |                  ✓                   | ✓ `d`, `U`, `v`, `x` |                                                                                                                                                                                                                              |
| Download queue: pause, resume, cancel                             |    ✓    |            ✓ `install …`             | ✓ `Space`, `c`, `X`  |                                                                                                                                                                                                                              |
| Choose a library folder; see free space                           |    ✓    |      ✓ `--library`, `libraries`      |   ✓ install picker   |                                                                                                                                                                                                                              |
| Move an install; add an existing install                          |    ✓    |     ✓ `move`, `relink`, `import`     |   ✓ `M`, `K`, `N`    |                                                                                                                                                                                                                              |
| Create a new library folder                                       |    —    |                  —                   |          —           | partly: `steam_library` may be any folder, which gets gaki's own layout ([CONFIG.md](CONFIG.md#library-layout)); other targets need a `steamapps` folder already, and gaki says to add the drive in Steam's Storage settings |
| Download bandwidth limit, download schedule, auto-update windows  |    —    |                  —                   |          —           | not planned. The TUI checks for updates after each library load; the daemon can resume interrupted installs (`install-resume auto`)                                                                                          |
| Enable or disable DLC                                             |    ✓    |        ✓ `enable`, `disable`         |        ✓ `D`         |                                                                                                                                                                                                                              |
| Beta branches                                                     |    ✓    |      ✓ `branches`, `set-branch`      |        ✓ `b`         |                                                                                                                                                                                                                              |
| Older builds (Steam: only through the console's `download_depot`) |    ✓    |  ✓ `manifests`, `downgrade`, `pin`   |        ✓ `V`         |                                                                                                                                                                                                                              |
| Depot file browser                                                |    ✓    | ✓ `depots files`, `depots download`  |   ✓ `o` → `Enter`    |                                                                                                                                                                                                                              |
| Move a library to another machine                                 |    ✓    | ✓ `library export`, `library import` |   ✓ `X` → Transfer   |                                                                                                                                                                                                                              |
| Shader pre-caching                                                |    —    |                  —                   |          —           | not done                                                                                                                                                                                                                     |

### 3.4 Play

| Official client                                             | libgaki |               gaki-cli               |       gaki-tui       | Gap                                                                                                                                                                                             |
| ----------------------------------------------------------- | :-----: | :----------------------------------: | :------------------: | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Launch native and Windows (Proton) builds                   |    ✓    |               ✓ `play`               | ✓ Play, **Play as…** |                                                                                                                                                                                                 |
| Launch options                                              |    ✓    | ✓ `launch-options`, `launch-targets` | ✓ `L`, **Play as…**  |                                                                                                                                                                                                 |
| Per-game compatibility tool, launch arguments, environment  |    ✓    |                 part                 |         part         | `config game` sets the profile, platform, wrappers and Steam integration; the TUI's **Game settings** all but the wrappers. Arguments and environment are set by hand in `games.toml`. not done |
| Install official Proton builds                              |    ✓    |              ✓ `proton`              |        ✓ `P`         | Third-party builds: non-goal (gaki finds the installed ones)                                                                                                                                    |
| Running games; stop one                                     |    ✓    |         ✓ `running`, `stop`          |        ✓ `R`         |                                                                                                                                                                                                 |
| Steam Cloud saves, with conflict choice                     |    ✓    |              ✓ `cloud`               |        ✓ `C`         |                                                                                                                                                                                                 |
| Steamworks in games, DRM, overlay, Steam Input, Remote Play |    —    |                 part                 |         part         | needs Steam: `play --steam` bridges the game to the running client. `drm` checks the tickets headlessly                                                                                         |
| Unlock achievements, game stats                             |    —    |                  —                   |          —           | Only a running game reports these, through Steamworks. needs Steam                                                                                                                              |

### 3.5 Store

| Official client                                                    | libgaki |           gaki-cli            |     gaki-tui     | Gap                 |
| ------------------------------------------------------------------ | :-----: | :---------------------------: | :--------------: | ------------------- |
| Search, deals, events                                              |    ✓    | ✓ `search`, `deals`, `events` |      ✓ `J`       |                     |
| A game's price (any region), players, news, reviews, similar games |    ✓    |               ✓               |      ✓ `Y`       |                     |
| Wishlist: view, add, remove                                        |    ✓    |         ✓ `wishlist`          | ✓ `J` → Wishlist |                     |
| A friend's wishlist                                                |    ✓    |      ✓ `wishlist <user>`      |        —         | not done (CLI only) |
| Buy, gift, refund, wallet top-up, points shop                      |    —    |               —               |        —         | non-goal            |
| Write a review                                                     |    —    |               —               |        —         | not done            |
| Localized store                                                    |    —    |               —               |        —         | non-goal            |

### 3.6 Community

| Official client                                             | libgaki |        gaki-cli         |   gaki-tui    | Gap                                              |
| ----------------------------------------------------------- | :-----: | :---------------------: | :-----------: | ------------------------------------------------ |
| Friends list, add, remove                                   |    ✓    |       ✓ `friends`       | ✓ Friends tab |                                                  |
| One-to-one chat                                             |    ✓    |        ✓ `chat`         |  ✓ `c`, `t`   |                                                  |
| Online status                                               |    ✓    |   ✓ `config presence`   |  ✓ settings   | Online or invisible only; no away/busy. not done |
| Group chats, voice chat                                     |    —    |            —            |       —       | not done                                         |
| View a profile                                              |    ✓    |        ✓ `user`         |  ✓ `v`, `/`   |                                                  |
| Edit your profile                                           |    —    |            —            |       —       | not done                                         |
| Workshop: browse, subscribe, install, update, rate, comment |    ✓    |      ✓ `workshop`       |     ✓ `W`     |                                                  |
| Inventory, wallet balance                                   |    ✓    | ✓ `inventory`, `wallet` |  ✓ `I`, `w`   |                                                  |
| Community Market: prices, search, your listings             |    ✓    |       ✓ `market`        |  ✓ `m`, `S`   |                                                  |
| Market buying and selling, trading, crafting                |    —    |            —            |       —       | non-goal                                         |
| Discussions, groups, guides, broadcasts                     |    —    |            —            |       —       | not done                                         |

### 3.7 Only in gaki

Not in the official client: a scriptable CLI (`--json` everywhere,
`capabilities` for drivers, a session daemon), Proton profiles shared with
lootrs, per-game wrappers, pinning a build against updates, the
`duplicates` scan, `available`, and SteamDB, ProtonDB and Proton-issue links.

### 3.8 Docs against the code

| Finding                                                                                                                                                                                                                                                                                   | Status |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| `docs/GAKI-CLI.md` left out `config get`, `unset`, `info-cache-ttl`, `proxy`, `experimental` and `clear-games`, `collections delete` and `rename`, and `workshop list`, `uninstall` and `unsubscribe`. All are now listed. (`appinfo` stays undocumented: it's a hidden maintainer tool.) | Fixed  |
| `docs/FEATURES.md` didn't list `library export`/`import` for the CLI, or game info `n`, SteamDB `Z`, the settings tabs, the Installs Duplicates tab and the user profile `v` / `/` for the TUI. Added under the features that gate them.                                                  | Fixed  |
| `--help`, `capabilities` and the TUI help come from the same definitions the code uses (clap and the keymap), so they can't drift from the commands.                                                                                                                                      | OK     |
