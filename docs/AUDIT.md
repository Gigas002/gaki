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

| Area | Finding | Status |
| ---- | ------- | ------ |
| Refresh and web tokens | Only in the OS keyring (Secret Service) or, with `secret_store = "memory"` / `GAKI_SECRET_STORE=memory`, in process memory. `account.toml` names the account and which secrets exist, never their values. | OK |
| Web cookie | `WebSession` keeps `steamLoginSecure` in a `SecretString`: redacted in `Debug`, zeroed on drop. | OK |
| Logs | No `tracing` or console line prints a token, password, cookie or refresh token; errors are logged by their message only. The daemon logs no request frames. | OK |
| Logout | `logout` revokes the refresh token at Steam and reports when it couldn't. | OK |
| Memory store | `Memory` holds secrets as plain `String`s, so they aren't zeroed on drop. It's opt-in and lives as long as the process anyway. | Accepted (low) |

### 1.2 Daemon socket

`$XDG_RUNTIME_DIR/gaki/daemon.sock`: the directory is created `0700` (or
tightened, and refused if another user owns it), the socket is `0600`, every
connection's peer uid is checked with `SO_PEERCRED`, and there is no `/tmp`
fallback. Frames are length-prefixed and capped at 64 MiB. A `--socket`
override is still `0600` and peer-checked. OK.

### 1.3 Untrusted input

| Area | Finding | Severity | Status |
| ---- | ------- | -------- | ------ |
| Depot chunks | A downloaded chunk was written without checking it against the manifest: its SHA-1 and size were only compared when re-verifying files already on disk. A corrupt or substituted chunk (or one longer than its slot, overwriting the next) went to disk as is. `get_chunk` now takes the manifest's `ChunkData` and refuses a chunk whose size or SHA-1 differs; the refusal is retried like a corrupt transfer. | medium | Fixed |
| Chunk decompression | The VZ (LZMA) decoder subtracted header sizes from the data length before checking it (an underflow on short data) and pre-allocated the size its footer claims (up to 4 GiB). The VSZ (Zstandard) and ZIP paths read without limit. All three now stop at the manifest's size for the chunk, and VZ checks its length first. Chunks are encrypted with the depot key, so forging one needs that key. | medium | Fixed |
| Manifest ZIP | A zipped manifest was unzipped without limit (a decompression bomb from a CDN server, or over plain HTTP). Now capped at 512 MiB; real manifests are tens of MB. | medium | Fixed |
| Cloud ZIP | A Cloud download that Steam stored zipped was unzipped without limit, and its buffer pre-allocated from a size field Steam controls. Now capped at 1 GiB, and the size field is only a bounded hint. | medium | Fixed |
| Manifest paths | Every entry's path and symlink target is checked before anything is created (`gaki-steam-cdn::cdn::paths`): no absolute paths, no `..` out of the install directory, no writing through an existing symlink, links pointing outside are skipped. Windows `\` separators are normalised before the check. | — | OK |
| Manifest signatures | Manifests aren't signature-checked (the key and scheme aren't known yet). With chunks now hash-checked, a forged manifest can still rearrange or drop files of the depot, not inject new content without the depot key. | low | Follow-up 8.16 |
| CDN transport | HTTPS whenever any content server offers it; plain HTTP only when none does. | — | OK |
| Cloud file names | `%Root%` tokens map to fixed directories, and the rest is joined with empty, `.` and `..` components dropped, so a crafted name can't leave its root. | — | OK |
| `installdir` | Every PICS or `appmanifest` install directory is checked to be one plain name before use, except in `import_install`. It now checks too. | low | Fixed |
| Launch entries | The executable and working directory of a PICS launch entry are joined to the install directory as given. Running a Valve launch entry trusts Valve as much as running the game's binary does. | low | Accepted |
| protobuf (RUSTSEC-2024-0437) | Stack overflow on deeply nested messages. `steam-vent` pins `=3.5.1`. The `deny.toml` note said the input was only CM traffic; depot manifests from CDN servers are parsed with it too. The worst case is a crash. Note corrected. | low | Accepted |
| lzma-rs | Unmaintained (no release since 2023); used only for VZ chunks, whose input is now bounded and decrypted with the depot key first. | low | Follow-up 8.17 |
| Config and profile files | The user's own files; parse errors become `Issue`s or load errors, never panics. | — | OK |
| Panics on input | No `unwrap`/`expect` on outside data in the parsers; string slicing on Steam text stays on character boundaries; the chat wrapper guards a zero width. | — | OK |

### 1.4 Processes and `unsafe`

| Area | Finding | Severity | Status |
| ---- | ------- | -------- | ------ |
| Launch diagnostics | `LaunchContext` carried a raw `*mut LaunchVerification` into the pipeline's own state, with hand-written `unsafe impl Send`/`Sync`. A `Clone`d context outliving the pipeline, or sent to another thread, would write through a dangling or shared pointer. Replaced by an `Arc<Mutex<LaunchVerification>>` the stages copy back; the `unsafe impl`s are gone. | medium | Fixed |
| Browser launchers | `xdg-open` was handed whatever URL it got. All of gaki's URLs are built from ids except a user profile's page, which comes from Steam's profile data; a `file:` URL or a `.desktop` path could make `xdg-open` run something. The TUI opener and the OpenID helper now accept only `http(s)://` URLs. | low | Fixed |
| Other spawns | `proton run`, wrappers and `env` come from the user's own config; `steam -silent` and the chat window run fixed programs with fixed or validated arguments (a friend's name is passed as `--name=…`). | — | OK |
| Remaining `unsafe` | `std::env::set_var` for the proxy (documented to run before any thread starts), `libc::getuid`, `libc::kill`, and `setsid` in a `pre_exec` (async-signal-safe). | — | OK |
