# Changelog

All notable changes to this project are documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Blue CSV releases three artifacts from a single version bump: the Zed extension (WASM crate at the repo root), the `bluecsv` library + CLI (crates.io), and the `bluecsv-ls` language server (crates.io). Entries tag the affected artifact when a change is scoped to one.

## [Unreleased]

### Added
- **bluecsv**: per-column type inference (`int` / `float` / `date` / `string`) and `summarize` (min / max / sum / mean / count / distinct) as a pure library API; dependency-free date recognition covers ISO-ish shapes.
- **bluecsv CLI**: `bluecsv infer <path>` and `bluecsv stats <path> <col>` subcommands.
- **bluecsv-ls**: hover on a data cell now shows the column's inferred type; hover on a header cell (when `hasHeader=true`) shows a summary block (count, distinct, empty, min, max, sum, mean, mismatch count).
- **bluecsv-ls**: type-mismatch diagnostic — cells whose type differs from the inferred column type are flagged (default severity: warning).
- **bluecsv-ls**: new workspace command `bluecsv.columnSummary` returns column stats as JSON.
- **bluecsv-ls**: new settings `bluecsv.inferTypes` (default `true`) and `bluecsv.typeMismatchSeverity` (default `"warning"`; accepts `"warning"`, `"hint"`, `"off"`).
- **bluecsv**: `bluecsv::stream` module with `stream_align` (two-pass, `Read + Seek`) and `stream_unalign` (single-pass, any `Read`); output is byte-identical to the in-memory paths.
- **bluecsv CLI**: `--stream` / `--no-stream` flags and `BLUECSV_STREAM_THRESHOLD` env var. Files ≥ 10 MB are streamed by default; stdin always buffers. Keeps peak memory bounded on large inputs.
- **bluecsv-ls**: new setting `bluecsv.maxBufferBytes` (default `10485760` = 10 MB). Above the cap, type inference and `alignOnSave` are skipped; basic diagnostics and completions still run. Set to `0` to disable the cap.
- **bluecsv**: criterion benchmark suite (`cargo bench -p bluecsv`) covering `parse`, `align`, and `infer_table` at 1k / 10k / 100k rows; 1M-row runs opt-in via `BLUECSV_BENCH_HUGE=1`.
- **bluecsv-ls**: end-to-end integration test that spawns the server binary and speaks JSON-RPC over stdio, asserting `hover`, `completion`, and `bluecsv.columnSummary` responses.

### Changed
- **extension**: rainbow-column scopes swap `@property` → `@attribute` and `@operator` → `@enum` so Gruvbox and Solarized users see more distinct column colors. See `plan/accessibility-audit.md` for the per-theme breakdown.
- **README**: consolidated settings table covers every `bluecsv.*` key, and the CLI section documents the streaming threshold.

## [0.6.3] - 2026-04-21

### Added
- **extension**: auto-download `bluecsv-ls` from GitHub Releases on first use, pinned to the extension's own version. Previously required the binary on `PATH`.
- **release pipeline**: `just publish VERSION [TITLE]` accepts an optional title that becomes the tag annotation and GitHub Release name; defaults to `bluecsv vX.Y.Z`.

### Changed
- **extension**: unsupported platforms (Windows, Linux-ARM, Intel Linux ≠ x86_64) now surface a clear "install from source with `cargo install bluecsv-ls`" message.
- **CI**: extension WASM crate now covered by `cargo fmt --check` and `cargo clippy -D warnings`.

### Fixed
- **release pipeline**: GitHub Release title now pulls from the tag annotation instead of the commit subject (`fetch-tags: true` + `git for-each-ref`).

## [0.6.2] - 2026-04-21

### Added
- First public release of all three artifacts.
- **bluecsv**: library + CLI with `align` / `unalign`.
- **bluecsv-ls**: CSV language server with column-aware completions, hover (column name + row index), diagnostics (row-width mismatches, unterminated quotes), code actions, and workspace commands for align, unalign, add/delete column, duplicate row, sort by column, cell navigation, and markdown-table round-trip.
- **extension**: Zed extension manifest + language config + tree-sitter queries for rainbow-column highlighting.
- **release pipeline**: `just publish VERSION` syncs versions across `Cargo.toml`, `extension.toml`, and `server/Cargo.toml`, tags, and pushes; CI builds `bluecsv-ls` for three targets and cuts a GitHub Release.
- **release pipeline**: `just publish-crates` publishes `bluecsv` then `bluecsv-ls` to crates.io.

[Unreleased]: https://github.com/anistark/bluecsv/compare/v0.6.3...HEAD
[0.6.3]: https://github.com/anistark/bluecsv/compare/v0.6.2...v0.6.3
[0.6.2]: https://github.com/anistark/bluecsv/releases/tag/v0.6.2
