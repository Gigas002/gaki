# gaki-steam-vent-crypto

The cryptography a Steam login needs, with nothing else attached:

- `PasswordKey`: RSA-encrypts the password with the key Steam hands out for
  the account (PKCS#1 v1.5).
- `SharedSecret`: Steam Guard mobile codes (TOTP) from a hex or base64 shared
  secret. Its `Debug` output is redacted.
- `MachineId`: the machine-ID message a logon carries, derived from a stable
  identifier or random.

## Origin

gaki's own copy of three MIT-licensed crates; see [LICENSE-MIT](LICENSE-MIT):

- [steam-vent-crypto](https://github.com/icewind1991/steam-vent) 0.2.1 by
  Robin Appelman,
- [another-steam-totp](https://github.com/juliarose/another-steam-totp) 0.4.2
  by Julia Rose,
- [steam-machine-id](https://github.com/juliarose/steam-machine-id) 0.2.0 by
  Julia Rose.

The modified crate is distributed under gaki's GPL-3.0-only.

## What changed

- `rsa` 0.10 instead of 0.9, which fixes the Marvin timing attack
  (RUSTSEC-2023-0071). gaki only encrypts with a public key, so it was never
  reachable, but the advisory ignore is gone.
- The three crates are one, with one error type (`CryptoError`). A bad
  public key is an error rather than an `expect` panic.
- The channel-encryption half of steam-vent-crypto (AES session keys,
  symmetric encrypt/decrypt, signature checks for the TCP transport) is
  dropped with that transport; the websocket transport is TLS.
- TOTP keeps only what a login uses: the code for now or for a given time.
  The confirmation-key, device-ID and server-time helpers (and their HTTP
  client) are dropped.
- The machine ID is written into a plain `Vec<u8>` (a fixed 155-byte
  message), without `bytes`.
- Dependencies come from `[workspace.dependencies]`; tests cover the known
  TOTP and machine-ID vectors and an encrypt/decrypt round trip with a fixed
  test key.
