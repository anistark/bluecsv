set shell := ["bash", "-cu"]

default:
    @just --list

# Run local checks (mirrors what CI does). Run before tagging a release.
check:
    cargo fmt --check --manifest-path server/Cargo.toml
    cargo clippy --all-targets --manifest-path server/Cargo.toml -- -D warnings
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

# Sync, commit, tag, push. CI builds binaries + cuts the GitHub Release. Usage: just publish 0.6.2
publish VERSION:
    @test -z "$(git status --porcelain)" || (echo "working tree dirty — commit first" && exit 1)
    just sync-version {{VERSION}}
    just check
    git commit -am "Bump to v{{VERSION}}"
    git tag -a v{{VERSION}} -m "bluecsv v{{VERSION}}"
    git push origin main
    git push origin v{{VERSION}}

# Publish bluecsv + bluecsv-ls to crates.io. Requires `cargo login`. Run AFTER `just publish`.
publish-crates:
    cargo publish --manifest-path server/bluecsv/Cargo.toml
    @echo "waiting 60s for crates.io index to propagate…"
    sleep 60
    cargo publish --manifest-path server/bluecsv-ls/Cargo.toml
