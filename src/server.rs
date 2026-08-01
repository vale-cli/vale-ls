use std::env;
use std::path::{Path, PathBuf};

use dashmap::DashMap;
use ropey::Rope;
use serde_json::Value;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::ini;
use crate::styles;
use crate::utils;
use crate::vale;
use crate::yml;

#[derive(Debug, Clone)]
struct TextDocumentItem {
    uri: Url,
    text: String,
}

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
    pub document_map: DashMap<String, Rope>,
    pub param_map: DashMap<String, Value>,
    pub cli: vale::ValeManager,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        self.set_roots(Backend::workspace_roots(&params));
        self.init(params.initialization_options).await;
        Ok(InitializeResult {
            server_info: None,
            offset_encoding: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(true),
                        })),
                        will_save: None,
                        will_save_wait_until: None,
                    },
                )),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: Default::default(),
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["cli.sync".to_string(), "cli.compile".to_string()],
                    work_done_progress_options: Default::default(),
                }),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: None,
                    work_done_progress_options: Default::default(),
                    all_commit_characters: None,
                    completion_item: None,
                }),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                        work_done_progress_options: WorkDoneProgressOptions {
                            work_done_progress: None,
                        },
                        resolve_provider: None,
                    },
                )),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(true),
                }),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        if self.should_sync() {
            self.do_sync().await;
        }
        self.client
            .log_message(MessageType::INFO, "initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            text: params.text_document.text,
        })
        .await
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        self.update(TextDocumentItem {
            uri: params.text_document.uri,
            text: std::mem::take(&mut params.content_changes[0].text),
        });
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        if params.text.is_some() {
            self.on_change(TextDocumentItem {
                uri: params.text_document.uri,
                text: params.text.unwrap(),
            })
            .await
        }
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<Value>> {
        match params.command.as_str() {
            "cli.sync" => self.do_sync().await,
            "cli.compile" => self.do_compile(params.arguments).await,
            _ => {}
        };
        Ok(None)
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = params.text_document.uri;
        let ext = self.get_ext(uri.clone());

        let text = self.document_map.get(uri.as_str());

        if ext == "yml" && text.is_some() {
            let rule = yml::Rule::new(uri.to_file_path().unwrap().to_str().unwrap());
            if rule.is_ok() {
                let link = rule.unwrap().source();
                let text = text.unwrap();

                let target = Url::parse(link.as_str());
                if target.is_err() {
                    self.client
                        .show_message(MessageType::ERROR, "link has Invalid URL")
                        .await;
                    return Ok(None);
                }

                let mut links = Vec::new();
                for (i, line) in text.lines().enumerate() {
                    let candidate = line.as_str();
                    if candidate.is_none() {
                        continue;
                    }
                    let lt = candidate.unwrap();
                    let sp = lt.find(link.as_str());
                    if sp.is_some() {
                        let start = Position::new(i as u32, sp.unwrap() as u32);
                        let end = Position::new(i as u32, link.len() as u32 + sp.unwrap() as u32);
                        links.push(DocumentLink {
                            range: Range::new(start, end),
                            target: Some(target.unwrap()),
                            tooltip: None,
                            data: None,
                        });

                        break;
                    }
                }

                return Ok(Some(links));
            }
        }

        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;

        let ext = self.get_ext(uri.clone());
        if self.document_map.get(uri.as_str()).is_none() {
            return Ok(None);
        }
        let pos = params.text_document_position_params.position;

        let rope = self.document_map.get(uri.as_str()).unwrap();
        let span = utils::position_to_range(pos, &rope);

        if span.is_none() {
            return Ok(None);
        }
        let range = span.unwrap();

        let token = utils::range_to_token(range, &rope);
        if ext == "ini" && ini::key_to_info(&token).is_some() {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: ini::key_to_info(&token).unwrap().to_string(),
                }),
                range: Some(range),
            }));
        } else if ext == "yml" && uri.to_file_path().is_ok() {
            let rule = yml::Rule::new(uri.to_file_path().unwrap().to_str().unwrap());
            if rule.is_ok() {
                let info = rule.unwrap();
                let desc = info.token_info(&token);
                if desc.is_some() {
                    return Ok(Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: desc.unwrap().to_string(),
                        }),
                        range: Some(range),
                    }));
                }
            }
        }

        Ok(None)
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        self.parse_params(Backend::settings_object(params.settings));
        self.client
            .log_message(MessageType::INFO, "configuration changed!")
            .await;
    }

    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        let mut roots = self.roots();

        for folder in params.event.removed {
            if let Ok(path) = folder.uri.to_file_path() {
                roots.retain(|root| Path::new(root) != path);
            }
        }

        for folder in params.event.added {
            if let Ok(path) = folder.uri.to_file_path() {
                let path = path.to_string_lossy().to_string();
                if !roots.contains(&path) {
                    roots.push(path);
                }
            }
        }

        self.set_roots(roots);
        self.client
            .log_message(MessageType::INFO, "workspace folders changed!")
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;

        let ext = self.get_ext(uri.clone());
        if self.document_map.get(uri.as_str()).is_none() {
            return Ok(None);
        }

        let position = params.text_document_position.position;
        let rope = self.document_map.get(uri.as_str()).unwrap();

        let context = rope.line(position.line as usize);
        let line = context.as_str().to_owned().unwrap_or("");

        let config = self.vale().config(self.config_path(), self.root_for(&uri));
        if config.is_err() {
            return Ok(None);
        }

        let styles = config.unwrap().styles_paths();
        match ext.as_str() {
            "ini" => match ini::complete(line, styles).await {
                Ok(computed) => {
                    return Ok(Some(CompletionResponse::Array(computed)));
                }
                Err(err) => {
                    self.client
                        .log_message(MessageType::ERROR, format!("Error: {}", err))
                        .await;
                }
            },
            "yml" => {
                let rule = yml::Rule::new(uri.to_file_path().unwrap().to_str().unwrap());
                if rule.is_ok() {
                    match rule.unwrap().complete(line) {
                        Ok(computed) => {
                            return Ok(Some(CompletionResponse::Array(computed)));
                        }
                        Err(err) => {
                            self.client
                                .log_message(MessageType::ERROR, format!("Error: {}", err))
                                .await;
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(None)
    }

    async fn code_lens(&self, _: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        Ok(None)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        if params.context.diagnostics.is_empty() {
            return Ok(None);
        }

        let diagnostics = params.context.diagnostics[0].data.as_ref();
        if diagnostics.is_none() {
            // TODO: What case is this?
            //
            // See https://github.com/ChrisChinchilla/vale-vscode/issues/48
            return Ok(None);
        }

        let s = serde_json::to_string(diagnostics.unwrap()).unwrap();
        match self.vale().fix(&s) {
            Ok(fixed) => {
                let alert: vale::ValeAlert = serde_json::from_str(&s).unwrap();
                let mut range = utils::alert_to_range(alert.clone());

                if !alert.action.name.is_some() {
                    return Ok(None);
                }

                let action_name = alert.action.name.unwrap();
                if action_name == "remove" {
                    // NOTE: we need to add a character when deleting to avoid
                    // leaving a double space.
                    range.end.character += 1;
                }

                let mut fixes = vec![];
                for fix in fixed.suggestions {
                    fixes.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: utils::make_title(
                            action_name.clone(),
                            alert.matched.clone(),
                            fix.clone(),
                        ),
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: Some(params.context.diagnostics.clone()),
                        edit: Some(WorkspaceEdit {
                            changes: Some(
                                [(
                                    params.text_document.uri.clone(),
                                    vec![TextEdit {
                                        range: range,
                                        new_text: fix,
                                    }],
                                )]
                                .iter()
                                .cloned()
                                .collect(),
                            ),
                            ..WorkspaceEdit::default()
                        }),
                        ..CodeAction::default()
                    }));
                }
                Ok(Some(fixes))
            }
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("Error: {}", e))
                    .await;
                Ok(None)
            }
        }
    }
}

impl Backend {
    async fn on_change(&self, params: TextDocumentItem) {
        let uri = params.uri.clone();
        let fp = uri.to_file_path();

        let vale = self.vale();
        let has_cli = vale.is_installed();

        self.update(params.clone());
        if has_cli && fp.is_ok() {
            match vale.run(fp.unwrap(), self.config_path(), self.config_filter()) {
                Ok(result) => {
                    let mut diagnostics = Vec::new();
                    for (_, v) in result.iter() {
                        for alert in v {
                            diagnostics.push(utils::alert_to_diagnostic(alert));
                        }
                    }
                    self.client
                        .publish_diagnostics(params.uri.clone(), diagnostics, None)
                        .await;
                }
                Err(err) => {
                    self.client
                        .log_message(MessageType::ERROR, format!("Parsing error: {:?}", err))
                        .await;
                    match serde_json::from_str::<vale::ValeError>(&err.to_string()) {
                        Ok(parsed) => {
                            self.client.show_message(MessageType::ERROR, parsed).await;
                        }
                        Err(e) => {
                            self.client.show_message(MessageType::ERROR, e).await;
                        }
                    };
                }
            }
        } else if !has_cli {
            // Say which binary is missing: a configured path that doesn't
            // exist is a typo to fix, not a missing install.
            let message = match vale.custom_exe {
                Some(exe) => format!("Configured Vale binary not found: {}", exe.display()),
                None => "Vale CLI not installed!".to_string(),
            };
            self.client.log_message(MessageType::WARNING, message).await;
        } else {
            self.client
                .log_message(MessageType::INFO, "No file path found. Is the file saved?")
                .await;
        }
    }

    async fn init(&self, params: Option<Value>) {
        self.parse_params(params);
        if self.should_install() {
            match self.vale().install_or_update() {
                Ok(status) => {
                    self.client.log_message(MessageType::INFO, status).await;
                }
                Err(err) => {
                    self.client
                        .show_message(MessageType::INFO, err.to_string())
                        .await;
                    self.client
                        .log_message(MessageType::ERROR, err.to_string())
                        .await;
                }
            }
        }
    }

    fn should_install(&self) -> bool {
        self.get_setting("installVale") == Some(Value::Bool(true))
    }

    /// `vale` returns the CLI to run, honoring a client-configured binary.
    ///
    /// The client can send `valeBinaryPath` at any point -- in
    /// `initializationOptions` or later, via `didChangeConfiguration` -- so we
    /// resolve it per call rather than at startup.
    fn vale(&self) -> vale::ValeManager {
        let configured = self.get_string("valeBinaryPath");
        if configured.is_empty() {
            return self.cli.clone();
        }
        self.cli.pinned_to(Some(PathBuf::from(configured)))
    }

    fn config_path(&self) -> String {
        self.get_string("configPath")
    }

    fn config_filter(&self) -> String {
        self.get_string("filter")
    }

    fn should_sync(&self) -> bool {
        self.get_setting("syncOnStartup") == Some(Value::Bool(true))
    }

    /// `root_path` returns the directory we run Vale from when we have no
    /// document to resolve against (e.g., a sync on startup).
    fn root_path(&self) -> String {
        let explicit = self.get_string("root");
        if !explicit.is_empty() {
            return explicit;
        }
        self.roots().first().cloned().unwrap_or_default()
    }

    /// `root_for` returns the directory we run Vale from for a given document.
    ///
    /// A multi-root workspace may hold projects with different `.vale.ini`
    /// files, so we resolve against the folder the document lives in -- the
    /// innermost one, if they're nested.
    fn root_for(&self, uri: &Url) -> String {
        match uri.to_file_path() {
            Ok(path) => self.root_for_path(&path),
            Err(_) => self.root_path(),
        }
    }

    fn root_for_path(&self, path: &Path) -> String {
        let explicit = self.get_string("root");
        if !explicit.is_empty() {
            return explicit;
        }

        Backend::best_root(&self.roots(), path).unwrap_or_else(|| self.root_path())
    }

    /// `best_root` returns the innermost of `roots` that contains `path`.
    fn best_root(roots: &[String], path: &Path) -> Option<String> {
        roots
            .iter()
            .filter(|root| path.starts_with(root))
            .max_by_key(|root| root.len())
            .cloned()
    }

    fn roots(&self) -> Vec<String> {
        match self.get_setting("roots") {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn set_roots(&self, roots: Vec<String>) {
        self.param_map.insert(
            "roots".to_string(),
            Value::Array(roots.into_iter().map(Value::String).collect()),
        );
    }

    /// `workspace_roots` returns the directories the client opened.
    ///
    /// `rootUri` is deprecated and not every client sends it, so we prefer
    /// `workspaceFolders` and fall back to our own working directory --
    /// leaving this empty makes every Vale invocation fail to spawn.
    fn workspace_roots(params: &InitializeParams) -> Vec<String> {
        let from_folders: Vec<PathBuf> = params
            .workspace_folders
            .iter()
            .flatten()
            .filter_map(|f| f.uri.to_file_path().ok())
            .collect();

        if !from_folders.is_empty() {
            return from_folders
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect();
        }

        params
            .root_uri
            .as_ref()
            .and_then(|uri| uri.to_file_path().ok())
            .or_else(|| env::current_dir().ok())
            .map(|path| vec![path.to_string_lossy().to_string()])
            .unwrap_or_default()
    }

    /// `settings_object` unwraps the settings a client pushed to us.
    ///
    /// Some clients send our settings verbatim; others scope them under a
    /// section name.
    fn settings_object(settings: Value) -> Option<Value> {
        let map = match settings {
            Value::Object(map) => map,
            _ => return None,
        };

        if let Some(scoped @ Value::Object(_)) = map.get("vale") {
            return Some(scoped.clone());
        }

        Some(Value::Object(map))
    }

    fn parse_params(&self, params: Option<Value>) {
        if let Some(Value::Object(map)) = params {
            for (k, v) in map {
                self.param_map.insert(k.to_string(), v.clone());
            }
        }
    }

    fn get_string(&self, key: &str) -> String {
        if self.get_setting(key).is_some() {
            let value = self.get_setting(key).unwrap();
            if value.is_string() {
                return value.as_str().unwrap().to_string();
            }
        }
        "".to_string()
    }

    fn get_setting(&self, key: &str) -> Option<Value> {
        if self.param_map.contains_key(key) {
            let value = self.param_map.get(key).unwrap();
            return Some(value.clone());
        }
        None
    }

    fn update(&self, params: TextDocumentItem) {
        let uri = params.uri.clone();
        if self.get_ext(uri) != "" {
            let rope = ropey::Rope::from_str(&params.text);
            self.document_map
                .insert(params.uri.to_string(), rope.clone());
        }
    }

    fn get_ext(&self, uri: Url) -> String {
        let ext = uri.path().split('.').last().unwrap_or("");
        if uri.path().contains(".vale.ini") {
            return "ini".to_string();
        } else if ext == "yml" {
            let config = self.vale().config(self.config_path(), self.root_for(&uri));
            if config.is_ok() {
                let styles = config.unwrap().styles_paths();
                let p = styles::StylesPath::new(styles);
                if p.has(uri.path()).unwrap_or(false) {
                    return "yml".to_string();
                }
            }
        }
        "".to_string()
    }

    async fn do_sync(&self) {
        match self.vale().sync(self.config_path(), self.root_path()) {
            Ok(_) => {
                self.client
                    .show_message(MessageType::INFO, "Successfully synced Vale config.")
                    .await;
            }
            Err(e) => {
                self.client
                    .show_message(MessageType::ERROR, format!("Failed to sync CLI: {}", e))
                    .await;
            }
        }
    }

    async fn do_compile(&self, arguments: Vec<Value>) {
        if arguments.len() == 0 {
            self.client
                .show_message(MessageType::ERROR, "No URI provided. Please try again.")
                .await;
            return;
        }

        let arg = arguments[0].as_str().unwrap().to_string();
        let uri = Url::parse(&arg).unwrap().to_file_path().unwrap();

        let ext = uri.extension().unwrap().to_str().unwrap();
        if ext != "yml" {
            self.client
                .show_message(
                    MessageType::ERROR,
                    "Only YAML files are supported; skipping compilation.",
                )
                .await;
            return;
        }

        let resp = self.vale().upload_rule(
            self.config_path(),
            self.root_for_path(&uri),
            uri.to_str().unwrap().to_string(),
        );

        match resp {
            Ok(r) => {
                let session = format!("https://regex101.com/r/{}", r.permalink_fragment);
                match open::that(session) {
                    Ok(_) => {
                        self.client
                            .show_message(
                                MessageType::INFO,
                                "Successfully compiled rule. Opening Regex101.",
                            )
                            .await;
                    }
                    Err(e) => {
                        self.client
                            .show_message(
                                MessageType::ERROR,
                                format!("Failed to open Regex101: {}", e),
                            )
                            .await;
                    }
                }
            }
            Err(e) => {
                self.client
                    .show_message(MessageType::ERROR, format!("Failed to compile rule: {}", e))
                    .await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    fn folder(path: &str) -> WorkspaceFolder {
        WorkspaceFolder {
            uri: Url::from_directory_path(path).unwrap(),
            name: path.to_string(),
        }
    }

    /// Directory URLs round-trip with a trailing separator, which `Path`
    /// ignores but `String` doesn't.
    fn as_paths(params: &InitializeParams) -> Vec<PathBuf> {
        Backend::workspace_roots(params)
            .iter()
            .map(PathBuf::from)
            .collect()
    }

    #[test]
    fn roots_prefer_workspace_folders() {
        let params = InitializeParams {
            root_uri: Some(Url::from_directory_path("/one").unwrap()),
            workspace_folders: Some(vec![folder("/two"), folder("/three")]),
            ..InitializeParams::default()
        };

        assert_eq!(
            as_paths(&params),
            vec![Path::new("/two"), Path::new("/three")]
        );
    }

    #[test]
    fn roots_fall_back_to_root_uri() {
        let params = InitializeParams {
            root_uri: Some(Url::from_directory_path("/one").unwrap()),
            workspace_folders: Some(vec![]),
            ..InitializeParams::default()
        };

        assert_eq!(as_paths(&params), vec![Path::new("/one")]);
    }

    #[test]
    fn roots_fall_back_to_our_cwd() {
        let roots = Backend::workspace_roots(&InitializeParams::default());
        let cwd = env::current_dir().unwrap().to_string_lossy().to_string();

        // A client that sends neither still has to leave us somewhere to run.
        assert_eq!(roots, vec![cwd]);
    }

    #[test]
    fn innermost_root_wins() {
        let roots = vec!["/work".to_string(), "/work/docs".to_string()];

        assert_eq!(
            Backend::best_root(&roots, Path::new("/work/docs/guide.md")),
            Some("/work/docs".to_string())
        );
        assert_eq!(
            Backend::best_root(&roots, Path::new("/work/README.md")),
            Some("/work".to_string())
        );
        assert_eq!(
            Backend::best_root(&roots, Path::new("/elsewhere/a.md")),
            None
        );
        // `/workshop` isn't inside `/work`.
        assert_eq!(
            Backend::best_root(&roots, Path::new("/workshop/a.md")),
            None
        );
    }

    #[test]
    fn settings_may_be_scoped() {
        let scoped = json!({"vale": {"configPath": "/a/.vale.ini"}});
        assert_eq!(
            Backend::settings_object(scoped),
            Some(json!({"configPath": "/a/.vale.ini"}))
        );

        let flat = json!({"configPath": "/a/.vale.ini"});
        assert_eq!(Backend::settings_object(flat.clone()), Some(flat));

        assert_eq!(Backend::settings_object(json!("nonsense")), None);
    }
}
