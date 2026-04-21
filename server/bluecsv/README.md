# bluecsv

CSV transforms (align / unalign, column ops, markdown-table round-trip) used by the [Blue CSV](https://github.com/anistark/bluecsv) Zed extension.

This crate is the library plus a `bluecsv` CLI. The language server lives in [`bluecsv-ls`](https://crates.io/crates/bluecsv-ls); the Zed extension lives in the [Blue CSV repo](https://github.com/anistark/bluecsv).

## CLI

```sh
cargo install bluecsv
bluecsv align input.csv
bluecsv align input.csv | bluecsv unalign -
```

## License

MIT. See [LICENSE](https://github.com/anistark/bluecsv/blob/main/LICENSE) in the main repo.
