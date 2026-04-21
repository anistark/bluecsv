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

## Conduct

Be kind. Disagree on the idea, not the person. Maintainers reserve the right to lock threads that stop being productive.
