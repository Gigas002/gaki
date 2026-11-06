# gaki-steam-cdn

Steam content-delivery client: depot manifests, depot keys, and chunk
downloads. It is the download engine behind [`libgaki`](../libgaki)'s installs,
updates and verification.

It is based on [steam-cdn](https://github.com/OccamyLab/steam-cdn) by
OccamyLab, by way of
[aurelia-steam-cdn](https://github.com/Drackrath/aurelia-steam-cdn), the fork
Aurelia's author made of it. Compared to upstream steam-cdn it:

- runs on steam-vent 0.5 and reqwest 0.13, with protos from `steam-vent-proto`;
- reads Steam's VSZ (Zstandard) chunk containers as well as VZ (LZMA) and zip;
- downloads chunks in parallel (16 in flight per depot), spreading them over a
  pool of content servers with failover, retries and backoff;
- reuses a chunk already on disk only when its SHA-1 matches;
- creates directories, empty files and symlinks from depot entries, and sets
  the executable bit;
- reports typed disk-full, offline and cancelled errors, and progress;
- keeps every manifest entry inside the install directory: absolute and `..`
  paths are refused, symlinks pointing outside are skipped, and nothing is
  written through a link (gaki's addition; upstream trusts the manifest).

gaki adopted the fork into its workspace to keep its dependencies current
(zip 9, ruzstd 0.9, the RustCrypto 0.9/0.5 crates, edition 2024). Every
source file carries a notice that it was modified.

## License

GPL-3.0-or-later, like the rest of gaki; see [`LICENSE`](../LICENSE).

Upstream steam-cdn is licensed under the Apache License 2.0, and its license
text is kept in [`LICENSE-APACHE`](LICENSE-APACHE).
