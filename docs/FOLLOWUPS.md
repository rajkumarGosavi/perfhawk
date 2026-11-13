# Phase 0 follow-ups

Items found during Phase 0 reviews and deliberately deferred. None blocks Phase 0.

## Track for Phase 1
- **Flame payload depth.** `min_fraction` limits the tree's width, not its depth. A few heavy, deep recursions produce flame JSON over 5 MiB, and deep trees can also exceed the browser's canvas size limit, so the graph renders blank. Add a depth cap or recursion collapsing to the flame API, plus a "too deep" notice in the UI.
- **Ungated query paths.** Filtered flame/top, `inverted_flame` and `sandwich` still walk every sample (about 213 M frames on the 500 MB benchmark) and are not covered by the perf gate. Extend the trie fast path to them, or gate them.
- **CI.** The 3-OS matrix and `cargo deny` have only run locally on Windows. Push the branch and get a green run (this also checks that `stacker` builds on Linux and macOS).
- **`prism rm`.** Pair it with repairing a corrupt or partial blob on re-import, and add an fsync before the blob rename.
- **Data directory.** Consider `%LOCALAPPDATA%` instead of Roaming `%APPDATA%` on Windows for multi-hundred-MB blobs.

## Before the first binary release
- Add a third-party notices file covering the Rust crates (e.g. cargo-about) and the npm production dependencies. The embedded UI bundle currently drops React's MIT notice.
- Add `X-Frame-Options: DENY` / `frame-ancestors 'none'` to server responses.

## Hardening / polish
- **Importer validation:** speedscope negative weights, non-monotonic `at` values and weights-length mismatches are accepted silently. Collapsed lines with a trailing space are rejected.
- **Gzip decompression is unbounded** (decompression-bomb risk). Fine under the local-only trust model; revisit before any remote access or share server.
- **Fx hashing is not DoS-resistant** against adversarial ids. Same local-trust caveat.
- **Server:** no single-flight on cache misses (two loads of the same id can double peak memory). Mutex-poison `unwrap`. Path rejections and 405 responses are plain text. The 2 GiB upload is buffered in memory. The 413 path is untested.
- **UI:** no `AbortController` for superseded requests. The canvas is not keyboard accessible (the Top table gives a keyboard path). The tooltip can clip near edges. No component or interaction tests.
- **CLI:** `to_sig3` rounds ties half-to-even, unlike JS `toPrecision` (cosmetic). Ctrl-C exits 0. The open-test timeout message omits the child's stderr.
- **Store:** the WAL pragma runs before the schema-version gate. The platform branches of `default_home` are untested.
- **Misc:** `$profile` loop variable in `examples/dotnet-demo/profile.ps1` shadows PowerShell's `$PROFILE`. Clippy `needless_lifetimes` in `crates/prism-import/tests/pprof.rs`. Recursive derived `Debug`/`Clone`/`PartialEq` on `FlameNode` overflow the stack on very deep trees (tests only).
