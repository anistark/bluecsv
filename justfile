set shell := ["bash", "-cu"]

default:
    @just --list

# Run local checks (mirrors what CI does). Run before tagging a release.
check:
    cargo fmt --check --all --manifest-path server/Cargo.toml
    cargo fmt --check
    cargo clippy --all-targets --manifest-path server/Cargo.toml -- -D warnings
    cargo clippy --target wasm32-wasip1 --release -- -D warnings
    cargo test --manifest-path server/Cargo.toml
    cargo build --target wasm32-wasip1 --release

# Install bluecsv-ls into ~/.cargo/bin for local Zed dev-install.
install-ls:
    cargo install --path server/bluecsv-ls

# Sync version across Cargo.toml, extension.toml, server/Cargo.toml (both package + bluecsv dep).
sync-version VERSION:
    sed -i.bak -E 's/^version = "[^"]*"/version = "{{VERSION}}"/' Cargo.toml extension.toml server/Cargo.toml
    rm -f Cargo.toml.bak extension.toml.bak server/Cargo.toml.bak
    cargo update --manifest-path server/Cargo.toml --workspace
    cargo update --manifest-path Cargo.toml --workspace

# Sync, commit, tag, push. Optional TITLE becomes the tag annotation + GitHub Release name.
# Usage: just publish 0.6.3   |   just publish 0.6.3 "Auto-download LSP binaries"
publish VERSION TITLE="":
    @test -z "$(git status --porcelain)" || (echo "working tree dirty — commit first" && exit 1)
    just sync-version {{VERSION}}
    just check
    git commit -am "Bump to v{{VERSION}}"
    title='{{TITLE}}'; git tag -a v{{VERSION}} -m "${title:-bluecsv v{{VERSION}}}"
    git push origin main
    git push origin v{{VERSION}}

# Publish bluecsv + bluecsv-ls to crates.io. Requires `cargo login`. Run AFTER `just publish`.
publish-crates:
    cargo publish --manifest-path server/bluecsv/Cargo.toml
    @echo "waiting 60s for crates.io index to propagate…"
    sleep 60
    cargo publish --manifest-path server/bluecsv-ls/Cargo.toml
