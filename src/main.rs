use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::path::Path;

use lazy_static::lazy_static;
use lsp_types::ImplementationProviderCapability;
use lsp_types::OneOf;
use lsp_types::TypeDefinitionProviderCapability;
use lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, GotoDefinitionParams,
    GotoDefinitionResponse, InitializeParams, InitializeResult, InitializedParams, MessageType,
    ServerCapabilities, ServerInfo,
};
use regex::Regex;
use serde_yaml::Value;
use tokio::fs;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::{LanguageServer, LspService, Server};

lazy_static! {
    static ref HELPERS_RE: Regex = Regex::new(r"\{\{-?\s*define\s+([^\}]+)\s*-?\}\}").unwrap();
    static ref STATEMENT_RE: Regex = Regex::new(r"\{\{-?\s*([^\}]+)\s*-?\}\}").unwrap();
    static ref RANGE_RE: Regex = Regex::new(r"range\s+(\$[\w]+),\s*(\$[\w]+)\s*:=").unwrap();
}

#[derive(Debug, Default)]
struct Chart {
    values: RwLock<serde_yaml::Value>,
    metadata: RwLock<serde_yaml::Value>,
}

// #[derive(Debug)]
struct Backend {
    client: tower_lsp::Client,
    chart: Chart,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Location {
    line: u32,
    range: (u32, u32),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Var {
    value: Value,
    location: Location,
}

#[derive(Debug, Clone)]
enum Statement {
    Global,
    IF(Location),
    ELSE(Location),
    WITH(Location),
    RANGE(Location),
}

#[allow(dead_code)]
#[derive(Debug)]
struct Scope {
    vars: BTreeMap<String, Var>,
    statement: Statement,
}

impl Scope {
    fn new(stat: Statement) -> Self {
        Self {
            vars: BTreeMap::new(),
            statement: stat,
        }
    }
}

#[derive(Debug)]
struct Context {
    scopes: VecDeque<Scope>,
}

#[allow(dead_code)]
impl Context {
    fn new(scope: Scope) -> Self {
        let mut scopes = VecDeque::new();
        scopes.push_back(scope);
        Self { scopes }
    }

    fn declare_var(&mut self, key: String, val: Var) -> Option<Var> {
        self.scopes.back_mut().unwrap().vars.insert(key, val)
    }

    fn set_var(&mut self, key: String, val: Var) -> Option<Var> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.vars.contains_key(&key) {
                return scope.vars.insert(key, val);
            }
        }
        None
    }

    fn get_var(&self, key: &String) -> Option<&Var> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.vars.get(key))
    }

    fn push_scope(&mut self, scope: Scope) {
        self.scopes.push_back(scope)
    }

    fn pop_scope(&mut self) -> Option<Scope> {
        self.scopes.pop_back()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        eprintln!("init params from client: {:?}", params.client_info);
        let result = InitializeResult {
            capabilities: ServerCapabilities {
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".into(), " ".into()]),
                    ..Default::default()
                }),
                text_document_sync: None,
                selection_range_provider: None,
                hover_provider: None,
                signature_help_provider: None,
                definition_provider: Some(OneOf::Left(true)),
                type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
                implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
                references_provider: None,
                document_highlight_provider: None,
                document_symbol_provider: None,
                workspace_symbol_provider: None,
                code_action_provider: None,
                code_lens_provider: None,
                document_formatting_provider: None,
                document_range_formatting_provider: None,
                document_on_type_formatting_provider: None,
                rename_provider: None,
                document_link_provider: None,
                color_provider: None,
                folding_range_provider: None,
                declaration_provider: None,
                execute_command_provider: None,
                workspace: None,
                call_hierarchy_provider: None,
                semantic_tokens_provider: None,
                moniker_provider: None,
                linked_editing_range_provider: None,
                experimental: None,
                inlay_hint_provider: None,
            },
            server_info: Some(ServerInfo {
                name: "helmls".into(),
                version: None,
            }),
            ..Default::default()
        };

        if let Some(url) = params.root_uri {
            let output = tokio::process::Command::new("helm")
                .arg("show")
                .arg("values")
                .arg(url.path())
                .output()
                .await
                .unwrap();

            let values: serde_yaml::Value = serde_yaml::from_slice(&output.stdout).unwrap();
            *self.chart.values.write().await = values;

            let output = tokio::process::Command::new("helm")
                .arg("show")
                .arg("chart")
                .arg(url.path())
                .output()
                .await
                .unwrap();

            let metadata: serde_yaml::Value = serde_yaml::from_slice(&output.stdout).unwrap();
            *self.chart.metadata.write().await = metadata;

            let output =
                tokio::fs::read_to_string(Path::new(url.path()).join("templates/_helpers.tpl"))
                    .await
                    .expect("read _helpers.tpl");

            let templates: Vec<&str> = HELPERS_RE
                .captures_iter(output.as_str())
                .filter_map(|cap| cap.get(1))
                .map(|mat| mat.as_str())
                .collect();

            for tmp in templates {
                eprintln!("cap: {}", tmp);
            }
        }

        Ok(result)
    }

    async fn initialized(&self, _params: InitializedParams) {
        eprintln!("initialized");
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        eprintln!("shutting down");
        Ok(())
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        eprintln!("complete: {:?}", params);
        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let path = params
            .text_document_position_params
            .text_document
            .uri
            .path();
        let mut lines = BufReader::new(fs::File::open(path).await.unwrap()).lines();

        let mut lineno = 0;

        let mut ctx = Context::new(Scope::new(Statement::Global));

        let position = &params.text_document_position_params.position;
        while let Some(line) = lines.next_line().await.unwrap() {
            if lineno == position.line {
                eprintln!("goto_definition: {:?}", position);
                let (start, _) = line
                    .chars()
                    .enumerate()
                    .take(position.character as usize)
                    .filter(|(_idx, c)| *c == ' ')
                    .last()
                    .unwrap();
                let (end, _) = line
                    .chars()
                    .enumerate()
                    .skip(position.character as usize)
                    .filter(|(_idx, c)| *c == ' ')
                    .next()
                    .unwrap();
                let key = &line[start + 1..end];
                eprintln!("ctx: {}, {:?}", key, ctx);
                if let Some(var) = ctx.get_var(&key.into()) {
                    eprintln!("definition: {:?}", var);
                    let res = Ok(Some(GotoDefinitionResponse::Scalar(lsp_types::Location {
                        uri: params
                            .text_document_position_params
                            .text_document
                            .uri
                            .clone(),
                        range: lsp_types::Range {
                            start: lsp_types::Position {
                                line: var.location.line,
                                character: var.location.range.0,
                            },
                            end: lsp_types::Position {
                                line: var.location.line,
                                character: var.location.range.1,
                            },
                        },
                    })));
                    eprintln!("goto: {:?}", res);
                    return res;
                } else {
                    eprintln!("not found");
                }

                return Ok(None);
            }
            for mat in STATEMENT_RE
                .captures_iter(line.as_str())
                .filter_map(|cap| cap.get(1))
            {
                let stat = mat.as_str();

                let location = Location {
                    line: lineno,
                    range: (mat.start() as u32, mat.end() as u32),
                };

                let tokens: Vec<&str> = stat.split_whitespace().collect();
                match tokens[..] {
                    [] => unreachable!(),
                    ["end", ..] => match ctx.pop_scope() {
                        Some(scope) => match scope.statement {
                            Statement::Global => unreachable!(),
                            _ => {
                                eprintln!("popped scope: {:?}", scope);
                            }
                        },
                        None => unreachable!(),
                    },
                    ["if", ..] => {
                        let scope = Scope::new(Statement::IF(location));
                        eprintln!("pushed if scope: {:?}", scope);
                        ctx.push_scope(scope);
                    }
                    ["else", ..] => {
                        match ctx.pop_scope() {
                            None => unreachable!(),
                            Some(scope) => match scope.statement {
                                Statement::IF(_) => {
                                    eprintln!("popped if statement: {:?}", scope);
                                }
                                _ => unreachable!(),
                            },
                        }
                        let scope = match tokens[1..] {
                            ["if", ..] => Scope::new(Statement::IF(location)),
                            _ => Scope::new(Statement::ELSE(location)),
                        };
                        eprintln!("pushed scope: {:?}", scope);
                        ctx.push_scope(scope);
                    }
                    ["with", ..] => {
                        let scope = Scope::new(Statement::WITH(location));
                        eprintln!("pushed scope: {:?}", scope);
                        ctx.push_scope(scope);
                    }
                    ["range", ..] => {
                        let scope = Scope::new(Statement::RANGE(location.clone()));
                        eprintln!("pushed scope: {:?}", scope);
                        ctx.push_scope(scope);

                        if let Some(cap) = RANGE_RE.captures(line.as_str()) {
                            for mat in cap.iter().skip(1) {
                                let mat = mat.unwrap();
                                eprintln!("declare var: {}", mat.as_str());
                                ctx.declare_var(
                                    mat.as_str().into(),
                                    Var {
                                        value: Value::Null,
                                        location: Location {
                                            line: lineno,
                                            range: (mat.start() as u32, mat.end() as u32),
                                        },
                                    },
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }

            lineno += 1;
        }
        Ok(None)
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        chart: Default::default(),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
