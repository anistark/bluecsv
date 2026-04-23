# Blue CSV

[![CI](https://github.com/anistark/bluecsv/actions/workflows/ci.yml/badge.svg)](https://github.com/anistark/bluecsv/actions/workflows/ci.yml)
[![crates.io: bluecsv](https://img.shields.io/crates/v/bluecsv?label=bluecsv)](https://crates.io/crates/bluecsv)
[![crates.io: bluecsv-ls](https://img.shields.io/crates/v/bluecsv-ls?label=bluecsv-ls)](https://crates.io/crates/bluecsv-ls)
[![Zed Extension](https://img.shields.io/badge/Zed-extension-084ccf)](https://zed.dev/extensions?query=bluecsv)
[![License: MIT](https://img.shields.io/badge/license-MIT-yellow.svg)](./LICENSE)

A [Zed](https://zed.dev) editor extension that makes CSV files feel like a spreadsheet — without leaving the text buffer.

> **Status:** pre-alpha. v0.6.0 adds markdown-table round-trip: `bluecsv.toMarkdownTable` renders the buffer as a GitHub-flavored pipe table, `bluecsv.fromMarkdownTable` parses one back. If a `plan/` directory is present in your checkout, it holds design docs and roadmap notes.

## What it does

Zed's extension API doesn't currently support custom file-renderers, so Blue CSV works with the text buffer you already have and makes it behave more like a grid:

- **Rainbow columns** — each column gets its own color, so rows and columns are scannable at a glance. Powered by [`tree-sitter-csv`](https://github.com/anistark/tree-sitter-csv).
- **Column alignment** — pad fields so columns line up in a monospace view. Toggleable.
- **Markdown-table round-trip** — convert `a,b,c` ↔ `| a | b | c |` and back.
- **Cell navigation** — Tab / Shift-Tab hop between fields; commands for add-column, delete-column, duplicate-row, header-aware sort.
- **Column-aware LSP** — diagnostics for row-width mismatches and unterminated quotes; completions drawn from values already seen in that column; hover shows `column name + row index`.
- **Type inference** — columns are classified as int, float, date, or string. Hover on a data cell shows the inferred type; hover on a header cell shows a summary (count, distinct, min, max, sum, mean). Outlier cells in typed columns surface as diagnostics.

Supports `.csv` and custom delimiters.

## Install

Not yet published to Zed's extensions registry. Until then, install it as a dev extension:

```sh
git clone https://github.com/anistark/bluecsv.git
```

In Zed, open the command palette and run **zed: install dev extension**, then point it at the cloned directory (the one containing `extension.toml`). The extension will auto-download a matching `bluecsv-ls` release on first use.

See [CONTRIBUTING.md](./CONTRIBUTING.md) for the full local-dev setup.

## LSP commands

Every transform is also surfaced as a **code action**: put your cursor in the file, open `editor: toggle code actions` (`cmd-.` by default), and pick one. Cell-scoped actions (delete column, duplicate row, sort by column) read the column / row from the cursor position.

The language server also exposes these raw `workspace/executeCommand` names, usable from keybindings or other clients:

| Command | Arguments | Effect |
| --- | --- | --- |
| `bluecsv.align` | `[uri]` | Pad every field with trailing spaces so columns line up. |
| `bluecsv.unalign` | `[uri]` | Strip alignment padding. |
| `bluecsv.addColumn` | `[{uri}]` | Append an empty column (with generated header if `hasHeader`). |
| `bluecsv.deleteColumn` | `[{uri, col}]` | Remove column at index `col`. |
| `bluecsv.duplicateRow` | `[{uri, row}]` | Duplicate row at index `row`. |
| `bluecsv.sortByColumn` | `[{uri, col, ascending?}]` | Sort rows by `col`; header row kept in place when `hasHeader`. |
| `bluecsv.nextCell` | `[{uri, position}]` | Request the editor move the cursor to the next cell. |
| `bluecsv.prevCell` | `[{uri, position}]` | Request the editor move the cursor to the previous cell. |
| `bluecsv.toMarkdownTable` | `[uri]` | Rewrite the buffer as a GitHub-flavored pipe table. |
| `bluecsv.fromMarkdownTable` | `[uri]` | Parse a pipe table back into CSV. |
| `bluecsv.columnSummary` | `[{uri, col}]` | Return `{type, confidence, count, distinct, min, max, sum, mean, mismatchRows}` as JSON. |

Zed extensions can't contribute keybindings directly — Tab / Shift-Tab cell navigation will land once the extension API grows a keymap hook, or you can wire the commands above manually in your Zed `keymap.json`.

## Alignment vs. markdown-table

Both commands make a CSV easier to read in a monospace buffer, but they serve different purposes:

- **Align** (`bluecsv.align`) pads fields with trailing spaces. The buffer is still a valid CSV — every downstream tool keeps working — and `bluecsv.unalign` restores the canonical form exactly.
- **Markdown table** (`bluecsv.toMarkdownTable`) rewrites the buffer as `| a | b |` rows with a `| --- |` separator. The output is no longer CSV; it's intended for previewing in Zed's markdown preview, pasting into docs, or diffing as prose. `bluecsv.fromMarkdownTable` reverses it.

Round-trip CSV → markdown → CSV is lossy for these cases:

- Leading / trailing whitespace inside a field is trimmed (markdown cells are whitespace-stripped).
- Fields that needed CSV quoting only for non-RFC reasons (e.g. a `|`) may come back unquoted — semantically equivalent, textually different.
- `\r` / `\n` inside a field are encoded as `<br>` in markdown and restored on the way back.

Simple canonical CSV (no padding inside fields) round-trips exactly.

## Type inference

Columns are classified by scanning all non-empty cells. A column is assigned a type (int, float, date, string) when at least **90%** of its non-empty cells match that type and at least **3** non-empty cells exist; otherwise it falls back to `string`. Numeric promotion: int cells satisfy a float column.

Recognized dates are ISO-ish: `YYYY-MM-DD`, `YYYY/MM/DD`, and `YYYY-MM-DDTHH:MM:SS` with optional `Z` or `±HH:MM` suffix. Ambiguous shapes like `MM/DD/YYYY` are left as strings.

## Settings

All settings live under the `bluecsv.*` namespace in Zed's `settings.json` (or `lsp.bluecsv.initialization_options`).

| Key | Type | Default | Effect |
| --- | --- | --- | --- |
| `hasHeader` | `bool` | `true` | Treat row 0 as a header. Affects sorting, completions, inference, and hover. |
| `alignOnSave` | `bool` | `false` | Run `bluecsv.align` automatically on save. |
| `inferTypes` | `bool` | `true` | Enable hover type labels, header summaries, and type-mismatch diagnostics. |
| `typeMismatchSeverity` | `"warning"` \| `"hint"` \| `"off"` | `"warning"` | Diagnostic level for cells that don't match their column's inferred type. |
| `maxBufferBytes` | `integer` | `10485760` (10 MB) | Size cap (in bytes) above which `inferTypes` and `alignOnSave` are skipped. Basic diagnostics (row-width, unterminated quotes) and completions still run. Set to `0` to disable the cap. |

## CLI

The `bluecsv` CLI exposes the library for use outside the editor.

```sh
bluecsv align data.csv              # pad columns to width
bluecsv unalign data.csv            # strip padding
bluecsv infer data.csv              # per-column: type, confidence, empty, mismatches
bluecsv stats data.csv 2            # min/max/sum/mean/count/distinct for column index 2
```

`align` and `unalign` stream the file through a two-pass algorithm when it exceeds 10 MB, keeping peak memory bounded. Override with `--stream` / `--no-stream`, or change the auto-detection threshold via the `BLUECSV_STREAM_THRESHOLD` environment variable (bytes). Stdin always buffers, because pipes can't be rewound for the second pass.

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md).

## License

[MIT](./LICENSE) © Kumar Anirudha.
