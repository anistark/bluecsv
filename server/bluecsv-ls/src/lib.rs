use std::collections::HashMap;

use dashmap::DashMap;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

pub mod diagnostics;
pub mod model;
pub mod transforms;

use model::Model;

const CMD_ALIGN: &str = "bluecsv.align";
const CMD_UNALIGN: &str = "bluecsv.unalign";
const CMD_ADD_COLUMN: &str = "bluecsv.addColumn";
const CMD_DELETE_COLUMN: &str = "bluecsv.deleteColumn";
const CMD_DUPLICATE_ROW: &str = "bluecsv.duplicateRow";
const CMD_SORT_BY_COLUMN: &str = "bluecsv.sortByColumn";
const CMD_NEXT_CELL: &str = "bluecsv.nextCell";
const CMD_PREV_CELL: &str = "bluecsv.prevCell";
const CMD_TO_MARKDOWN: &str = "bluecsv.toMarkdownTable";
const CMD_FROM_MARKDOWN: &str = "bluecsv.fromMarkdownTable";

#[derive(Debug, Deserialize, Clone)]
#[serde(default, rename_all = "camelCase")]
struct Config {
    align_on_save: bool,
    has_header: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            align_on_save: false,
            has_header: true,
        }
    }
}

pub struct Backend {
    client: Client,
    docs: DashMap<Url, String>,
    config: RwLock<Config>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            docs: DashMap::new(),
            config: RwLock::new(Config::default()),
        }
    }

    async fn publish_diagnostics(&self, uri: &Url, version: Option<i32>) {
        let text = match self.docs.get(uri) {
            Some(t) => t.clone(),
            None => return,
        };
        let diags = diagnostics::scan(&text);
        self.client
            .publish_diagnostics(uri.clone(), diags, version)
            .await;
    }

    async fn move_cell(&self, uri: Url, text: &str, pos: Position, forward: bool) {
        let model = Model::parse(text);
        let flat: Vec<&model::Cell> = model.cells.iter().flatten().collect();
        if flat.is_empty() {
            return;
        }
        let current = flat
            .iter()
            .position(|c| pos_in_range(pos, c.range))
            .or_else(|| {
                flat.iter()
                    .position(|c| cell_starts_after(c.range.start, pos))
                    .map(|i| if forward { i.saturating_sub(1) } else { i })
            });
        let target_idx = match (current, forward) {
            (Some(i), true) => i + 1,
            (Some(i), false) => i.saturating_sub(1),
            (None, true) => 0,
            (None, false) => flat.len() - 1,
        };
        let Some(target) = flat.get(target_idx) else {
            return;
        };
        let _ = self
            .client
            .show_document(ShowDocumentParams {
                uri,
                external: Some(false),
                take_focus: Some(true),
                selection: Some(target.range),
            })
            .await;
    }

    async fn set_config_from(&self, value: Value) {
        let section = value.get("bluecsv").cloned().unwrap_or(value);
        if let Ok(cfg) = serde_json::from_value::<Config>(section) {
            *self.config.write().await = cfg;
        }
    }
}

fn full_range(text: &str) -> Range {
    let mut line: u32 = 0;
    let mut last_line_start: usize = 0;
    for (i, c) in text.char_indices() {
        if c == '\n' {
            line += 1;
            last_line_start = i + 1;
        }
    }
    let tail = &text[last_line_start..];
    let col: u32 = tail.chars().map(|c| c.len_utf16() as u32).sum();
    Range {
        start: Position::new(0, 0),
        end: Position::new(line, col),
    }
}

fn extract_uri(args: &[Value]) -> Option<Url> {
    let arg = args.first()?;
    let raw = arg.as_str().map(|s| s.to_string()).or_else(|| {
        arg.get("uri")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    })?;
    Url::parse(&raw).ok()
}

fn arg_obj(args: &[Value]) -> Option<&serde_json::Map<String, Value>> {
    args.first()?.as_object()
}

fn arg_usize(args: &[Value], key: &str) -> Option<usize> {
    arg_obj(args)?
        .get(key)?
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
}

fn arg_bool(args: &[Value], key: &str) -> Option<bool> {
    arg_obj(args)?.get(key)?.as_bool()
}

fn arg_position(args: &[Value]) -> Option<Position> {
    let obj = arg_obj(args)?;
    let pos = obj.get("position")?;
    serde_json::from_value(pos.clone()).ok()
}

fn pos_in_range(pos: Position, range: Range) -> bool {
    let after_start = pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character);
    let before_end = pos.line < range.end.line
        || (pos.line == range.end.line && pos.character <= range.end.character);
    after_start && before_end
}

fn cell_starts_after(start: Position, pos: Position) -> bool {
    start.line > pos.line || (start.line == pos.line && start.character > pos.character)
}

fn invalid_params(msg: impl Into<std::borrow::Cow<'static, str>>) -> Error {
    Error {
        code: tower_lsp::jsonrpc::ErrorCode::InvalidParams,
        message: msg.into(),
        data: None,
    }
}

fn command_action(title: String, command: &str, arguments: Vec<Value>) -> CodeActionOrCommand {
    CodeActionOrCommand::CodeAction(CodeAction {
        title: title.clone(),
        kind: Some(CodeActionKind::REFACTOR),
        command: Some(Command {
            title,
            command: command.to_string(),
            arguments: Some(arguments),
        }),
        ..Default::default()
    })
}

fn build_code_actions(uri: &Url, model: &Model, pos: Position) -> Vec<CodeActionOrCommand> {
    let uri_str = uri.to_string();
    let uri_arg = Value::String(uri_str.clone());
    let uri_obj = serde_json::json!({ "uri": uri_str });

    let mut out = vec![
        command_action("Align columns".into(), CMD_ALIGN, vec![uri_arg.clone()]),
        command_action("Unalign columns".into(), CMD_UNALIGN, vec![uri_arg.clone()]),
        command_action("Add column".into(), CMD_ADD_COLUMN, vec![uri_obj.clone()]),
        command_action(
            "Convert CSV to markdown table".into(),
            CMD_TO_MARKDOWN,
            vec![uri_arg.clone()],
        ),
        command_action(
            "Parse markdown table into CSV".into(),
            CMD_FROM_MARKDOWN,
            vec![uri_arg],
        ),
    ];

    if let Some(cell) = model.cell_at(pos) {
        let col_label = model
            .header(cell.col)
            .filter(|h| !h.is_empty())
            .map(|h| format!("“{h}”"))
            .unwrap_or_else(|| format!("column {}", cell.col + 1));

        let col_arg = serde_json::json!({ "uri": uri_str, "col": cell.col });
        let row_arg = serde_json::json!({ "uri": uri_str, "row": cell.row });
        let sort_asc = serde_json::json!({ "uri": uri_str, "col": cell.col, "ascending": true });
        let sort_desc = serde_json::json!({ "uri": uri_str, "col": cell.col, "ascending": false });

        out.push(command_action(
            format!("Delete {col_label}"),
            CMD_DELETE_COLUMN,
            vec![col_arg],
        ));
        out.push(command_action(
            format!("Duplicate row {}", cell.row + 1),
            CMD_DUPLICATE_ROW,
            vec![row_arg],
        ));
        out.push(command_action(
            format!("Sort rows by {col_label} (ascending)"),
            CMD_SORT_BY_COLUMN,
            vec![sort_asc],
        ));
        out.push(command_action(
            format!("Sort rows by {col_label} (descending)"),
            CMD_SORT_BY_COLUMN,
            vec![sort_desc],
        ));
    }

    out
}

fn replace_edit(uri: &Url, old_text: &str, new_text: String) -> WorkspaceEdit {
    let edit = TextEdit {
        range: full_range(old_text),
        new_text,
    };
    WorkspaceEdit {
        changes: Some(HashMap::from([(uri.clone(), vec![edit])])),
        ..Default::default()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(opts) = params.initialization_options {
            self.set_config_from(opts).await;
        }

        let sync = TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(TextDocumentSyncKind::FULL),
            will_save: Some(false),
            will_save_wait_until: Some(true),
            save: Some(TextDocumentSyncSaveOptions::Supported(true)),
        };

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "bluecsv-ls".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(sync)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![
                        CMD_ALIGN.into(),
                        CMD_UNALIGN.into(),
                        CMD_ADD_COLUMN.into(),
                        CMD_DELETE_COLUMN.into(),
                        CMD_DUPLICATE_ROW.into(),
                        CMD_SORT_BY_COLUMN.into(),
                        CMD_NEXT_CELL.into(),
                        CMD_PREV_CELL.into(),
                        CMD_TO_MARKDOWN.into(),
                        CMD_FROM_MARKDOWN.into(),
                    ],
                    work_done_progress_options: Default::default(),
                }),
                completion_provider: Some(CompletionOptions::default()),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
                    first_trigger_character: "\"".into(),
                    more_trigger_character: None,
                }),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "bluecsv-ls initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.docs.insert(uri.clone(), params.text_document.text);
        self.publish_diagnostics(&uri, Some(params.text_document.version))
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if let Some(change) = params.content_changes.into_iter().next_back() {
            self.docs.insert(uri.clone(), change.text);
        }
        self.publish_diagnostics(&uri, Some(params.text_document.version))
            .await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.docs.remove(&params.text_document.uri);
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        if let Some(text) = params.text {
            self.docs.insert(params.text_document.uri.clone(), text);
        }
        self.publish_diagnostics(&params.text_document.uri, None)
            .await;
    }

    async fn will_save_wait_until(
        &self,
        params: WillSaveTextDocumentParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        if !self.config.read().await.align_on_save {
            return Ok(None);
        }
        let uri = &params.text_document.uri;
        let Some(text) = self.docs.get(uri).map(|t| t.clone()) else {
            return Ok(None);
        };
        let aligned = bluecsv::align(&text);
        if aligned == text {
            return Ok(None);
        }
        Ok(Some(vec![TextEdit {
            range: full_range(&text),
            new_text: aligned,
        }]))
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        self.set_config_from(params.settings).await;
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<Value>> {
        let uri = extract_uri(&params.arguments)
            .ok_or_else(|| invalid_params("expected uri argument"))?;
        let Some(text) = self.docs.get(&uri).map(|t| t.clone()) else {
            return Ok(None);
        };
        let has_header = self.config.read().await.has_header;

        let new_text = match params.command.as_str() {
            CMD_ALIGN => bluecsv::align(&text),
            CMD_UNALIGN => bluecsv::unalign(&text),
            CMD_ADD_COLUMN => transforms::add_column(&text, has_header),
            CMD_DELETE_COLUMN => {
                let col = arg_usize(&params.arguments, "col")
                    .ok_or_else(|| invalid_params("expected col argument"))?;
                transforms::delete_column(&text, col)
            }
            CMD_DUPLICATE_ROW => {
                let row = arg_usize(&params.arguments, "row")
                    .ok_or_else(|| invalid_params("expected row argument"))?;
                transforms::duplicate_row(&text, row)
            }
            CMD_SORT_BY_COLUMN => {
                let col = arg_usize(&params.arguments, "col")
                    .ok_or_else(|| invalid_params("expected col argument"))?;
                let ascending = arg_bool(&params.arguments, "ascending").unwrap_or(true);
                transforms::sort_by_column(&text, col, ascending, has_header)
            }
            CMD_TO_MARKDOWN => transforms::to_markdown_table(&text),
            CMD_FROM_MARKDOWN => transforms::from_markdown_table(&text),
            CMD_NEXT_CELL | CMD_PREV_CELL => {
                let position = arg_position(&params.arguments)
                    .ok_or_else(|| invalid_params("expected position argument"))?;
                let forward = params.command == CMD_NEXT_CELL;
                self.move_cell(uri.clone(), &text, position, forward).await;
                return Ok(None);
            }
            other => {
                return Err(Error {
                    code: tower_lsp::jsonrpc::ErrorCode::MethodNotFound,
                    message: format!("unknown command: {other}").into(),
                    data: None,
                })
            }
        };
        if new_text == text {
            return Ok(None);
        }
        let edit = replace_edit(&uri, &text, new_text);
        let _ = self.client.apply_edit(edit).await?;
        Ok(None)
    }

    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        if params.ch != "\"" {
            return Ok(None);
        }
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let Some(text) = self.docs.get(&uri).map(|t| t.clone()) else {
            return Ok(None);
        };
        let model = Model::parse(&text);
        let Some(cell) = model.cell_at(pos).cloned() else {
            return Ok(None);
        };
        // Only intervene when the cell is an unquoted field that has grown a
        // stray quote. A properly quoted field starts with `"` after trimming
        // leading padding; skip those.
        if cell.raw.trim_start().starts_with('"') || !cell.raw.contains('"') {
            return Ok(None);
        }
        let quoted = transforms::quote_field(&cell.raw);
        Ok(Some(vec![TextEdit {
            range: cell.range,
            new_text: quoted,
        }]))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let Some(text) = self.docs.get(&uri).map(|t| t.clone()) else {
            return Ok(None);
        };
        let model = Model::parse(&text);
        let Some(cell) = model.cell_at(pos).cloned() else {
            return Ok(None);
        };
        let skip_header = self.config.read().await.has_header;
        let values = model.column_values_excluding(cell.col, Some(cell.row), skip_header);
        let items: Vec<CompletionItem> = values
            .into_iter()
            .map(|v| CompletionItem {
                label: v,
                kind: Some(CompletionItemKind::VALUE),
                ..Default::default()
            })
            .collect();
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let Some(text) = self.docs.get(&uri).map(|t| t.clone()) else {
            return Ok(None);
        };
        let model = Model::parse(&text);
        let Some(cell) = model.cell_at(pos).cloned() else {
            return Ok(None);
        };
        let has_header = self.config.read().await.has_header;
        let header = if has_header {
            model.header(cell.col).map(|s| s.to_string())
        } else {
            None
        };
        let col_label = header
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| format!("column {}", cell.col + 1));
        let row_label = if has_header {
            if cell.row == 0 {
                "header".to_string()
            } else {
                format!("row {}", cell.row)
            }
        } else {
            format!("row {}", cell.row + 1)
        };
        let md = format!("**{col_label}** — {row_label}");
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: md,
            }),
            range: Some(cell.range),
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let Some(text) = self.docs.get(&uri).map(|t| t.clone()) else {
            return Ok(None);
        };
        let model = Model::parse(&text);
        let Some(cell) = model.cell_at(pos).cloned() else {
            return Ok(None);
        };
        if cell.value.is_empty() {
            return Ok(None);
        }
        let skip_header = self.config.read().await.has_header;
        let hits = model.find_in_column(cell.col, &cell.value, skip_header);
        let next = hits.into_iter().find(|c| c.row != cell.row);
        Ok(next.map(|c| {
            GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: c.range,
            })
        }))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let pos = params.range.start;
        let Some(text) = self.docs.get(&uri).map(|t| t.clone()) else {
            return Ok(None);
        };
        let model = Model::parse(&text);
        Ok(Some(build_code_actions(&uri, &model, pos)))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let Some(text) = self.docs.get(&uri).map(|t| t.clone()) else {
            return Ok(None);
        };
        let model = Model::parse(&text);
        let Some(cell) = model.cell_at(pos).cloned() else {
            return Ok(None);
        };
        if cell.value.is_empty() {
            return Ok(Some(Vec::new()));
        }
        let skip_header = self.config.read().await.has_header;
        let include_self = params.context.include_declaration;
        let locs: Vec<Location> = model
            .find_in_column(cell.col, &cell.value, skip_header)
            .into_iter()
            .filter(|c| include_self || c.row != cell.row)
            .map(|c| Location {
                uri: uri.clone(),
                range: c.range,
            })
            .collect();
        Ok(Some(locs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_range_single_line_no_newline() {
        let r = full_range("abc");
        assert_eq!(r.start, Position::new(0, 0));
        assert_eq!(r.end, Position::new(0, 3));
    }

    #[test]
    fn full_range_multiple_lines() {
        let r = full_range("a\nbc\nd");
        assert_eq!(r.end, Position::new(2, 1));
    }

    #[test]
    fn full_range_trailing_newline() {
        let r = full_range("a,b\n");
        assert_eq!(r.end, Position::new(1, 0));
    }

    #[test]
    fn extract_uri_from_string() {
        let args = vec![Value::String("file:///tmp/a.csv".into())];
        assert!(extract_uri(&args).is_some());
    }

    #[test]
    fn extract_uri_from_object() {
        let args = vec![serde_json::json!({ "uri": "file:///tmp/a.csv" })];
        assert!(extract_uri(&args).is_some());
    }

    fn action_titles(actions: &[CodeActionOrCommand]) -> Vec<String> {
        actions
            .iter()
            .map(|a| match a {
                CodeActionOrCommand::CodeAction(ca) => ca.title.clone(),
                CodeActionOrCommand::Command(c) => c.title.clone(),
            })
            .collect()
    }

    #[test]
    fn code_actions_always_offer_whole_buffer_commands() {
        let uri = Url::parse("file:///tmp/a.csv").unwrap();
        let model = Model::parse("id,name\n1,alice\n");
        let actions = build_code_actions(&uri, &model, Position::new(5, 5));
        let titles = action_titles(&actions);
        assert!(titles.iter().any(|t| t == "Align columns"));
        assert!(titles.iter().any(|t| t == "Unalign columns"));
        assert!(titles.iter().any(|t| t == "Add column"));
        assert!(titles.iter().any(|t| t == "Convert CSV to markdown table"));
        assert!(titles.iter().any(|t| t == "Parse markdown table into CSV"));
        assert_eq!(actions.len(), 5);
    }

    #[test]
    fn code_actions_add_cell_scoped_actions_when_cursor_in_cell() {
        let uri = Url::parse("file:///tmp/a.csv").unwrap();
        let model = Model::parse("id,name\n1,alice\n2,bob\n");
        let actions = build_code_actions(&uri, &model, Position::new(1, 3));
        let titles = action_titles(&actions);
        assert!(titles.iter().any(|t| t == "Delete “name”"));
        assert!(titles.iter().any(|t| t == "Duplicate row 2"));
        assert!(titles
            .iter()
            .any(|t| t == "Sort rows by “name” (ascending)"));
        assert!(titles
            .iter()
            .any(|t| t == "Sort rows by “name” (descending)"));
    }

    #[test]
    fn code_action_command_carries_uri_and_args() {
        let uri = Url::parse("file:///tmp/a.csv").unwrap();
        let model = Model::parse("id,name\n1,alice\n");
        let actions = build_code_actions(&uri, &model, Position::new(1, 3));
        let delete = actions
            .iter()
            .find_map(|a| match a {
                CodeActionOrCommand::CodeAction(ca)
                    if ca.command.as_ref().map(|c| c.command.as_str())
                        == Some(CMD_DELETE_COLUMN) =>
                {
                    ca.command.clone()
                }
                _ => None,
            })
            .expect("deleteColumn action missing");
        let args = delete.arguments.expect("args missing");
        let obj = args[0].as_object().unwrap();
        assert_eq!(obj["uri"], "file:///tmp/a.csv");
        assert_eq!(obj["col"], 1);
    }
}
