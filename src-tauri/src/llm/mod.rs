pub mod llamacpp;
pub mod ollama;
pub mod types;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::config::{AppConfig, BackendKind};
use crate::error::{AppError, AppResult};
use crate::llm::llamacpp::LlamaCppBackend;
use crate::llm::ollama::OllamaBackend;
use crate::llm::types::{BackendStatus, ChatRequest, TokenSink};

/// Выбранный на время запроса бэкенд.
pub enum Backend {
    Ollama(OllamaBackend),
    LlamaCpp(LlamaCppBackend),
}

impl Backend {
    pub async fn chat(&self, req: &ChatRequest) -> AppResult<String> {
        match self {
            Backend::Ollama(b) => b.chat(req).await,
            Backend::LlamaCpp(b) => {
                let mut req = req.clone();
                llamacpp::flatten_system_role(&mut req);
                b.chat(&req).await
            }
        }
    }

    pub async fn chat_stream(&self, req: &ChatRequest, sink: &TokenSink) -> AppResult<String> {
        match self {
            Backend::Ollama(b) => b.chat_stream(req, sink).await,
            Backend::LlamaCpp(b) => {
                let mut req = req.clone();
                llamacpp::flatten_system_role(&mut req);
                b.chat_stream(&req, sink).await
            }
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Backend::Ollama(_) => "ollama",
            Backend::LlamaCpp(_) => "llama_cpp",
        }
    }
}

/// Владеет процессом встроенного llama-server и решает, кто обслуживает запрос.
#[derive(Clone)]
pub struct LlmService {
    child: Arc<Mutex<Option<tokio::process::Child>>>,
    resource_dir: Option<PathBuf>,
}

impl LlmService {
    pub fn new(resource_dir: Option<PathBuf>) -> Self {
        Self { child: Arc::new(Mutex::new(None)), resource_dir }
    }

    fn resource_dir(&self) -> Option<&Path> {
        self.resource_dir.as_deref()
    }

    pub fn ollama(&self, cfg: &AppConfig) -> OllamaBackend {
        OllamaBackend::new(cfg.ollama.clone())
    }

    fn llama_cpp(&self, cfg: &AppConfig) -> LlamaCppBackend {
        LlamaCppBackend::new(cfg.llama_cpp.clone(), self.child.clone())
    }

    /// Состояние обоих бэкендов — интерфейс показывает их в статус-баре.
    pub async fn statuses(&self, cfg: &AppConfig) -> Vec<BackendStatus> {
        vec![
            self.ollama(cfg).status().await,
            self.llama_cpp(cfg).status(self.resource_dir()).await,
        ]
    }

    /// Гибридный выбор: в режиме `Auto` предпочитаем Ollama (она уже держит
    /// модель в VRAM и управляет выгрузкой), а если её нет — поднимаем свой сервер.
    pub async fn resolve(&self, cfg: &AppConfig) -> AppResult<Backend> {
        match cfg.backend {
            BackendKind::Ollama => Ok(Backend::Ollama(self.ollama(cfg))),
            BackendKind::LlamaCpp => {
                let backend = self.llama_cpp(cfg);
                backend.ensure_running(self.resource_dir()).await?;
                Ok(Backend::LlamaCpp(backend))
            }
            BackendKind::Auto => {
                let ollama = self.ollama(cfg);
                if ollama.status().await.available {
                    return Ok(Backend::Ollama(ollama));
                }
                let fallback = self.llama_cpp(cfg);
                fallback.ensure_running(self.resource_dir()).await.map_err(|e| {
                    AppError::BackendUnavailable(format!(
                        "Ollama не запущена, встроенный сервер тоже недоступен: {e}"
                    ))
                })?;
                Ok(Backend::LlamaCpp(fallback))
            }
        }
    }

    /// Убиваем дочерний llama-server при выходе, иначе он останется висеть в
    /// фоне и держать несколько гигабайт VRAM.
    pub async fn shutdown(&self) {
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.kill().await;
        }
    }
}
