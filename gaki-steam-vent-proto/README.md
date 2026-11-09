# gaki-steam-vent-proto

The Steam client protocol's protobuf messages, with the traits
`gaki-steam-vent` uses to send and route them (`RpcMessage`,
`RpcMessageWithKind`, `RpcMethod`, `MsgKindEnum`, ...).

## Origin

gaki's own copy of [steam-vent-proto](https://codeberg.org/steam-vent/proto)
0.5.2 by Robin Appelman, MIT-licensed; see [LICENSE-MIT](LICENSE-MIT). The
modified crate is distributed under gaki's GPL-3.0-only.

## What changed

- `steam-vent-proto`, `-common` and `-steam` are one crate. The game crates
  (`-tf2`, `-csgo`, `-dota2`, `-deadlock`) and the game-coordinator handshake
  type are dropped; gaki talks to no game coordinator.
- `protos/` is the `steam/` directory of
  [SteamDatabase/Protobufs](https://github.com/SteamDatabase/Protobufs) at
  `b78e8f1`, newer than 0.5.2's. To update, copy that directory over
  `protos/` and run the workspace tests.
- The bindings are no longer checked in. `build.rs` generates them from
  `protos/` on every clean build, with pure-Rust rust-protobuf codegen. It
  is a port of upstream's `steam-vent-proto-build` tool, without
  `syn`/`quote`/`prettyplease`.
- `protobuf` follows the workspace (>= 3.7.2) instead of upstream's `=3.5.1`
  pin, which fixes RUSTSEC-2024-0437 (unbounded recursion on deeply nested
  messages).
