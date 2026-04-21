# bluecsv-ls

Language server for the [Blue CSV](https://github.com/anistark/bluecsv) Zed extension. Provides:

- Diagnostics for row-width mismatches and unterminated quotes.
- Column-aware completions drawn from values already in that column.
- Hover with `column name + row index`.
- Code actions and commands for align / unalign, sort, add / delete column, duplicate row, markdown-table round-trip.

The Zed extension auto-downloads this binary on first use. Manual install:

```sh
cargo install bluecsv-ls
```

## License

MIT. See [LICENSE](https://github.com/anistark/bluecsv/blob/main/LICENSE) in the main repo.
