# gaki-steam-vent

A client for Steam's CM network: server discovery, password/QR/Steam Guard
login, the encrypted session, and typed RPC and notification messaging. With
the `chat` feature, a friend-chat client on top.

## Origin

gaki's own copy of
[aurelia-steam-vent](https://codeberg.org/Drackrath/aurelia-steam-vent) 0.7.1
(with `aurelia-steam-vent-core` 0.1.0), Aurelia's fork of
[steam-vent](https://codeberg.org/steam-vent/steam-vent) by Robin Appelman,
MIT-licensed; see [LICENSE-MIT](LICENSE-MIT). The `chat` module is
[aurelia-steam-vent-chat](https://codeberg.org/Drackrath/aurelia-steam-vent-chat)
0.8.0, GPL-3.0-or-later. The modified crate is distributed under gaki's
GPL-3.0-only.

## What changed

Breaking, against aurelia-steam-vent 0.7.1:

- Built on `gaki-steam-vent-proto`, so the protobuf runtime is the workspace's
  (>= 3.7.2, RUSTSEC-2024-0437) rather than `=3.5.1`, and on
  `gaki-steam-vent-crypto` (rsa 0.10, RUSTSEC-2023-0071) instead of
  `steam-vent-crypto`, `another-steam-totp` and `steam-machine-id`.
- Websocket only. The TCP transport and its channel encryption, and the game
  coordinator client, are gone; gaki uses neither.
- `aurelia-steam-vent-core` is the private `base` module (its public types are
  re-exported from the crate root as before); `aurelia-steam-vent-chat` is the
  `chat` module behind the `chat` feature.
- `FileGuardDataStore::new` takes the file's path; there is no default
  location under the user's cache dir any more. The file is created `0600`.
- `SharedSecretAuthConfirmationHandler::new` returns a `Result` instead of
  panicking on a bad secret, and `LoginError::InvalidPubKey` carries the
  crypto error. The console confirmation handler, `Connection::sender`, the
  server-side helpers (`is_server`, `with_app_id`) and the key-log file are
  removed.
- `job_multi` streams without `async-stream`, and a dropped stream
  unregisters its job.
- Dependencies come from `[workspace.dependencies]`; `binrw`, `bytemuck`,
  `byteorder`, `crc`, `directories`, `num-bigint-dig`, `num-traits` and
  `serde_with` are no longer needed. Examples, the Nix flake and the Wireshark
  tooling are not carried over.

Fixes:

- A Steam Guard code typed at the prompt is trimmed before it is sent.
- Protobuf headers write the `eresult` they carry, and a header cut short is
  an error rather than a shorter header.
- The non-protobuf header length is 36 bytes (the kind included).
- A non-positive heartbeat interval from the server falls back to the default
  instead of spinning.
- A closed unauthenticated connection reports end of stream.
- `Os` serializes through its enum mapping rather than an `as` cast.

## Tests

Unit tests run against an in-memory CM (`test_support`), no network: message
framing and headers, job and notification routing, logon and Steam Guard
confirmation, guard data, tokens, server lists and chat events.
