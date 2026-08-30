use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::config::LlamaCppConfig;
use crate::error::{AppError, AppResult};
use crate::llm::types::{BackendStatus, ChatRequest, Role, TokenSink};

/// Встроенный запасной бэкенд: сами поднимаем `llama-server` и говорим с ним
/// по OpenAI-совместимому API. Нужен, когда у пользователя нет Ollama.
pub struct LlamaCppBackend {
    cfg: LlamaCppConfig,
    http: reqwest::Client,
    child: Arc<Mutex<Option<tokio::process::Child>>>,
}

#[derive(Deserialize)]
struct CompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    #[serde(default)]
    message: Option<CompletionMessage>,
    #[serde(default)]
    delta: Option<CompletionMessage>,
}

#[derive(Deserialize)]
struct CompletionMessage {
    #[serde(default)]
    content: Option<String>,
}

impl LlamaCppBackend {
    pub fn new(cfg: LlamaCppConfig, child: Arc<Mutex<Option<tokio::process::Child>>>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .connect_timeout(Duration::from_secs(2))
            .build()
            .expect("не удалось создать HTTP-клиент");
        Self { cfg, http, child }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.cfg.port)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url(), path)
    }

    /// Ищем llama-server: сначала явный путь из настроек, потом каталог рядом
    /// с исполняемым файлом приложения (туда его кладёт установщик).
    fn resolve_binary(&self, resource_dir: Option<&Path>) -> Option<PathBuf> {
        if let Some(p) = &self.cfg.server_binary {
            return p.exists().then(|| p.clone());
        }
        let candidate = resource_dir?.join("llm").join("llama-server.exe");
        candidate.exists().then_some(candidate)
    }

    async fn is_alive(&self) -> bool {
        self.http
            .get(self.url("/health"))
            .timeout(Duration::from_millis(800))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    pub async fn status(&self, resource_dir: Option<&Path>) -> BackendStatus {
        let endpoint = self.base_url();

        if self.is_alive().await {
            return BackendStatus {
                kind: "llama_cpp".into(),
                available: true,
                version: None,
                endpoint,
                models: Vec::new(),
                vision_model_ready: self.cfg.mmproj_path.is_some(),
                text_model_ready: true,
                detail: "llama-server запущен".into(),
            };
        }

        let Some(_) = self.resolve_binary(resource_dir) else {
            return BackendStatus::unavailable(
                "llama_cpp",
                endpoint,
                "llama-server.exe не найден. Укажите путь в настройках или положите его в подкаталог llm рядом с приложением.".into(),
            );
        };
        let Some(model) = &self.cfg.model_path else {
            return BackendStatus::unavailable(
                "llama_cpp",
                endpoint,
                "Не указан GGUF-файл модели.".into(),
            );
        };
        if !model.exists() {
            return BackendStatus::unavailable(
                "llama_cpp",
                endpoint,
                format!("GGUF-файл не найден: {}", model.display()),
            );
        }

        BackendStatus {
            kind: "llama_cpp".into(),
            available: true,
            version: None,
            endpoint,
            models: vec![model.display().to_string()],
            vision_model_ready: self.cfg.mmproj_path.is_some(),
            text_model_ready: true,
            detail: "Готов к запуску (сервер стартует при первой генерации)".into(),
        }
    }

    /// Идемпотентный старт: если сервер уже отвечает — ничего не делаем.
    pub async fn ensure_running(&self, resource_dir: Option<&Path>) -> AppResult<()> {
        if self.is_alive().await {
            return Ok(());
        }

        let binary = self.resolve_binary(resource_dir).ok_or_else(|| {
            AppError::BackendUnavailable(
                "llama-server.exe не найден — укажите путь в настройках".into(),
            )
        })?;
        let model = self.cfg.model_path.as_ref().ok_or_else(|| {
            AppError::Config("не указан GGUF-файл модели для llama.cpp".into())
        })?;

        let mut cmd = tokio::process::Command::new(&binary);
        cmd.arg("--model")
            .arg(model)
            .arg("--port")
            .arg(self.cfg.port.to_string())
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--n-gpu-layers")
            .arg(self.cfg.gpu_layers.to_string())
            .arg("--ctx-size")
            .arg(self.cfg.context_size.to_string());

        if let Some(mmproj) = &self.cfg.mmproj_path {
            cmd.arg("--mmproj").arg(mmproj);
        }

        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW — иначе поверх приложения выскочит консоль.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let spawned = cmd
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| AppError::Backend(format!("не удалось запустить llama-server: {e}")))?;

        *self.child.lock().await = Some(spawned);

        // Загрузка весов в VRAM занимает секунды-десятки секунд.
        for _ in 0..120 {
            if self.is_alive().await {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        Err(AppError::Backend(
            "llama-server не поднялся за 60 секунд".into(),
        ))
    }

    fn body(&self, req: &ChatRequest, stream: bool) -> Value {
        let messages: Vec<Value> = req
            .messages
            .iter()
            .map(|m| {
                if m.images.is_empty() {
                    return json!({ "role": m.role.as_str(), "content": m.content });
                }
                // Мультимодальный формат OpenAI: массив частей вместо строки.
                let mut parts = vec![json!({ "type": "text", "text": m.content })];
                for img in &m.images {
                    parts.push(json!({
                        "type": "image_url",
                        "image_url": { "url": format!("data:image/jpeg;base64,{img}") }
                    }));
                }
                json!({ "role": m.role.as_str(), "content": parts })
            })
            .collect();

        let mut body = json!({
            "model": "local",
            "messages": messages,
            "stream": stream,
            "temperature": req.temperature,
            "top_p": req.top_p,
            "max_tokens": req.max_tokens,
        });
        if req.json_mode {
            body["response_format"] = json!({ "type": "json_object" });
        }
        body
    }

    pub async fn chat(&self, req: &ChatRequest) -> AppResult<String> {
        let resp = self
            .http
            .post(self.url("/v1/chat/completions"))
            .json(&self.body(req, false))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Backend(format!("llama-server ответил {status}: {text}")));
        }

        let parsed: CompletionResponse = resp.json().await?;
        Ok(parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.or(c.delta))
            .and_then(|m| m.content)
            .unwrap_or_default())
    }

    pub async fn chat_stream(&self, req: &ChatRequest, sink: &TokenSink) -> AppResult<String> {
        let resp = self
            .http
            .post(self.url("/v1/chat/completions"))
            .json(&self.body(req, true))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Backend(format!("llama-server ответил {status}: {text}")));
        }

        let mut stream = resp.bytes_stream();
        let mut full = String::new();
        let mut buffer = String::new();

        while let Some(item) = stream.next().await {
            buffer.push_str(&String::from_utf8_lossy(&item?));

            while let Some(idx) = buffer.find('\n') {
                let line: String = buffer.drain(..=idx).collect();
                let line = line.trim();
                // SSE: полезная нагрузка живёт в строках `data: ...`.
                let Some(payload) = line.strip_prefix("data:") else {
                    continue;
                };
                let payload = payload.trim();
                if payload == "[DONE]" {
                    return Ok(full);
                }
                let parsed: CompletionResponse = match serde_json::from_str(payload) {
                    Ok(p) => p,
                    // Служебные кадры (например, `usage`) игнорируем.
                    Err(_) => continue,
                };
                if let Some(text) = parsed
                    .choices
                    .into_iter()
                    .next()
                    .and_then(|c| c.delta.or(c.message))
                    .and_then(|m| m.content)
                {
                    if !text.is_empty() {
                        full.push_str(&text);
                        let _ = sink.send(text);
                    }
                }
            }
        }
        Ok(full)
    }
}

/// Не у всех сборок llama.cpp есть системный промпт как отдельная роль —
/// на всякий случай схлопываем его в первое пользовательское сообщение.
pub fn flatten_system_role(req: &mut ChatRequest) {
    if req.messages.len() < 2 || req.messages[0].role != Role::System {
        return;
    }
    let system = req.messages.remove(0);
    if let Some(first) = req.messages.first_mut() {
        first.content = format!("{}\n\n{}", system.content, first.content);
    }
}
