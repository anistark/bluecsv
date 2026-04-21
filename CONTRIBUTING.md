# Contributing to Blue CSV

Thanks for your interest. Blue CSV is early — most of the value right now is in design feedback, not pull requests.

## Before you open a PR

1. Read [`plan/overview.md`](./plan/overview.md) and [`plan/architecture.md`](./plan/architecture.md).
2. Check [`plan/roadmap.md`](./plan/roadmap.md) to see which version the work belongs to. PRs that jump ahead of the roadmap are likely to be deferred.
3. For anything non-trivial, open an issue first and sketch the approach. Saves churn.

## Project layout

```sh
bluecsv/
├── extension.toml     Zed extension manifest (pins grammar repo + rev)
├── Cargo.toml + src/  Rust extension crate (builds to WASM)
├── languages/csv/     language config + highlights
├── server/            Rust workspace: CLI + language server
├── fixtures/          sample files for smoke-testing
└── .github/workflows/ CI
```

The grammar lives in a separate repo, [anistark/tree-sitter-csv](https://github.com/anistark/tree-sitter-csv), and is fetched by Zed at dev-install time using the URL + `rev` pinned in `extension.toml`. Contributors to bluecsv never need to clone it.

The bluecsv repo has two independent Cargo workspaces: the root crate compiles to WASM for Zed to load the extension, and `server/` compiles natively for the `bluecsv` CLI and `bluecsv-ls` language server.

## Development setup

Prerequisites:

- [Zed](https://zed.dev).
- Rust toolchain with the `wasm32-wasip1` target (for the Zed extension and language server).

### Build and test the Rust workspace

```sh
cd server
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

The `bluecsv` crate ships a library used by the LSP plus a `bluecsv` CLI:

```sh
cargo run --bin bluecsv -- align ../fixtures/sample.csv
cargo run --bin bluecsv -- align ../fixtures/sample.csv | cargo run --bin bluecsv -- unalign -
```

The language server is `bluecsv-ls`. To make it discoverable by the Zed extension, build and put it on `PATH`:

```sh
cargo install --path bluecsv-ls
```

### Build the extension WASM

Zed builds the extension itself when you run **zed: install dev extension**, but you can verify locally:

```sh
cargo build --target wasm32-wasip1 --release
```

### Install the extension locally

1. In Zed, open the command palette and run **zed: install dev extension**.
2. Point it at this repo's root (the directory containing `extension.toml`).
3. Open `fixtures/sample.csv` to smoke-test the install — you should see CSV-mode highlighting.

Re-run **zed: install dev extension** after any change to `extension.toml`, the grammar, or the language files.

## Filing issues

- **Bugs:** include Zed version, OS, a minimal `.csv` that reproduces, and what you expected vs. saw.
- **Feature requests:** point to the roadmap version you think it belongs in, or argue for a new slot.
- **Design questions:** fine to open as issues while the design is still fluid.

## Pull requests

- One logical change per PR.
- Keep commit messages descriptive; link the issue.
- Tests required for anything in the LSP (parsing, diagnostics, transforms).
- No need to bump the version yourself — releases are cut from the roadmap.

## Code style

- Rust: `cargo fmt` + `cargo clippy -- -D warnings` must pass.
- No commentary on *what* the code does; comments only for non-obvious *why*.
- Prefer small, composable functions over clever ones.

## Releasing (maintainers)

The repo ships a [`justfile`](./justfile) that drives the release flow. Run `just --list` to see all recipes.

### Cut a release

```sh
just check                                        # local pre-flight, mirrors CI
just publish 0.6.3                                # default title "bluecsv v0.6.3"
just publish 0.6.3 "Auto-download LSP binaries"   # custom title
```

`just publish`:

1. Refuses if the working tree is dirty.
2. Syncs `X.Y.Z` across `Cargo.toml`, `extension.toml`, and `server/Cargo.toml` (both `workspace.package.version` and the `workspace.dependencies.bluecsv.version`), then refreshes both `Cargo.lock`s.
3. Re-runs `just check` on the bumped state.
4. Commits `"Bump to vX.Y.Z"`, creates the annotated tag, pushes `main` + tag.

The optional `TITLE` argument becomes the tag annotation and — via [`release.yml`](./.github/workflows/release.yml) — the GitHub Release name. Omit it to use the default `"bluecsv vX.Y.Z"`. Avoid single quotes inside the title (shell quoting); double quotes are fine.

### What happens after the tag push

`release.yml` triggers on `v*.*.*` tags and:

1. Builds `bluecsv-ls` for `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`.
2. Tarballs each as `bluecsv-ls-<target>.tar.gz` and creates a GitHub Release, using the tag annotation as the Release name and auto-generated notes for the body.

The extension's WASM code fetches these tarballs on first use (pinned to the tag matching the extension version), so binary availability on the Releases page is load-bearing — verify the Release looks right before moving on.

### crates.io (optional, manual)

```sh
just publish-crates
```

Publishes `bluecsv` then `bluecsv-ls`. Requires `cargo login` with a crates.io token. Run only after the GitHub Release above is confirmed good — crates.io publishes are irreversible.

### Grammar releases

The grammar lives in [anistark/tree-sitter-csv](https://github.com/anistark/tree-sitter-csv) and has its own release cycle. To consume a new grammar version, update the `rev` in `[grammars.csv]` in `extension.toml` and cut a new bluecsv release.

## Conduct

Be kind. Disagree on the idea, not the person. Maintainers reserve the right to lock threads that stop being productive.
