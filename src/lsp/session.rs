use std::collections::HashMap;

use serde_json::{json, Value};

use crate::clangd::ClangdProcess;
use crate::config::{ProjectPaths, WrapperConfig};
use crate::tasks::RestartTask;

#[derive(Debug, Clone)]
pub struct OpenDocument {
    pub uri: String,
    pub language_id: String,
    pub version: i32,
    pub text: String,
}

pub struct SharedState {
    pub user_args: Vec<String>,
    pub restart_tasks: Vec<Box<dyn RestartTask>>,
    pub wrapper_config: WrapperConfig,
    pub project: ProjectPaths,
    pub initialize_params: Option<Value>,
    pub initialized: bool,
    pub open_documents: HashMap<String, OpenDocument>,
    pub restart_generation: u64,
    pub next_internal_id: i64,
    pub restarting: bool,
    pub clangd: Option<ClangdProcess>,
}

impl SharedState {
    pub fn new(
        user_args: Vec<String>,
        restart_tasks: Vec<Box<dyn RestartTask>>,
        wrapper_config: WrapperConfig,
        project: ProjectPaths,
    ) -> Self {
        Self {
            user_args,
            restart_tasks,
            wrapper_config,
            project,
            initialize_params: None,
            initialized: false,
            open_documents: HashMap::new(),
            restart_generation: 0,
            next_internal_id: 10_000,
            restarting: false,
            clangd: None,
        }
    }

    pub fn observe_client_message(&mut self, message: &Value) {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return;
        };

        match method {
            "initialize" => {
                if let Some(params) = message.get("params") {
                    self.initialize_params = Some(params.clone());
                    self.initialized = false;
                }
            }
            "initialized" => {
                self.initialized = true;
            }
            "textDocument/didOpen" => {
                if let Some(params) = message.get("params") {
                    if let Some(doc) = parse_open_document(params) {
                        self.open_documents.insert(doc.uri.clone(), doc);
                    }
                }
            }
            "textDocument/didClose" => {
                if let Some(uri) = message
                    .get("params")
                    .and_then(|params| params.get("textDocument"))
                    .and_then(|doc| doc.get("uri"))
                    .and_then(Value::as_str)
                {
                    self.open_documents.remove(uri);
                }
            }
            "textDocument/didChange" => {
                if let Some(params) = message.get("params") {
                    update_open_document(self, params);
                }
            }
            "exit" => {}
            _ => {}
        }
    }

    pub fn replay_messages(&mut self) -> Vec<Value> {
        let mut messages = Vec::new();

        if let Some(params) = self.initialize_params.clone() {
            messages.push(json!({
                "jsonrpc": "2.0",
                "id": self.allocate_internal_id(),
                "method": "initialize",
                "params": params,
            }));
        }

        messages.push(json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {},
        }));

        for doc in self.open_documents.values() {
            messages.push(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": doc.uri,
                        "languageId": doc.language_id,
                        "version": doc.version,
                    },
                    "text": doc.text,
                }
            }));
        }

        self.initialized = true;
        messages
    }

    pub fn allocate_internal_id(&mut self) -> i64 {
        let id = self.next_internal_id;
        self.next_internal_id += 1;
        id
    }

    pub fn bump_restart_generation(&mut self) {
        self.restart_generation += 1;
    }
}

fn parse_open_document(params: &Value) -> Option<OpenDocument> {
    let text_document = params.get("textDocument")?;
    Some(OpenDocument {
        uri: text_document.get("uri")?.as_str()?.to_string(),
        language_id: text_document
            .get("languageId")
            .and_then(Value::as_str)
            .unwrap_or("cpp")
            .to_string(),
        version: text_document
            .get("version")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32,
        text: params
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

fn update_open_document(state: &mut SharedState, params: &Value) {
    let uri = params
        .get("textDocument")
        .and_then(|doc| doc.get("uri"))
        .and_then(Value::as_str);

    let version = params
        .get("textDocument")
        .and_then(|doc| doc.get("version"))
        .and_then(Value::as_i64);

    let Some(uri) = uri else {
        return;
    };

    let Some(doc) = state.open_documents.get_mut(uri) else {
        return;
    };

    if let Some(version) = version {
        doc.version = version as i32;
    }

    if let Some(changes) = params.get("contentChanges").and_then(Value::as_array) {
        if let Some(change) = changes.first() {
            if let Some(text) = change.get("text").and_then(Value::as_str) {
                doc.text = text.to_string();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_state() -> SharedState {
        SharedState::new(
            vec!["--background-index".to_string()],
            vec![],
            WrapperConfig {
                clangd_path: "clangd".to_string(),
                log_level: "error".to_string(),
                watch_root: PathBuf::from("."),
            },
            ProjectPaths::resolve(PathBuf::from("."), &[]),
        )
    }

    #[test]
    fn replay_includes_initialize_and_open_docs() {
        let mut state = test_state();
        state.initialize_params = Some(json!({"processId": 1}));
        state.open_documents.insert(
            "file:///main.cpp".to_string(),
            OpenDocument {
                uri: "file:///main.cpp".to_string(),
                language_id: "cpp".to_string(),
                version: 1,
                text: "int main() {}".to_string(),
            },
        );

        let replay = state.replay_messages();
        assert_eq!(replay.len(), 3);
        assert_eq!(replay[0]["method"], "initialize");
        assert_eq!(replay[1]["method"], "initialized");
        assert_eq!(replay[2]["method"], "textDocument/didOpen");
    }
}
