use std::time::Duration;

use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::OllamaConfig;
use crate::error::{AppError, AppResult};
use crate::llm::types::{BackendStatus, ChatRequest, TokenSink};

pub struct OllamaBackend {
    cfg: OllamaConfig,
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct VersionResponse {
    version: String,
}

#[derive(Deserialize)]
struct TagsResponse {
    models: Vec<TagEntry>,
}

#[derive(Deserialize)]
struct TagEntry {
    name: String,
}

#[derive(Deserialize)]
struct ChatChunk {
    #[serde(default)]
    message: Option<ChunkMessage>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
struct ChunkMessage {
    #[serde(default)]
    content: String,
}

impl OllamaBackend {
    pub fn new(cfg: OllamaConfig) -> Self {
        let http = reqwest::Client::builder()
            // Локальная 8B-модель на холодную может думать долго: первый запрос
            // включает загрузку весов в VRAM.
            .timeout(Duration::from_secs(600))
            .connect_timeout(Duration::from_secs(2))
            .build()
            .expect("не удалось создать HTTP-клиент");
        Self { cfg, http }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.cfg.base_url.trim_end_matches('/'), path)
    }

    pub async fn installed_models(&self) -> AppResult<Vec<String>> {
        let resp: TagsResponse = self.http.get(self.url("/api/tags")).send().await?.json().await?;
        Ok(resp.models.into_iter().map(|m| m.name).collect())
    }

    pub async fn status(&self) -> BackendStatus {
        let endpoint = self.cfg.base_url.clone();
        let version = match self.http.get(self.url("/api/version")).send().await {
            Ok(r) => match r.json::<VersionResponse>().await {
                Ok(v) => v.version,
                Err(e) => {
                    return BackendStatus::unavailable(
                        "ollama",
                        endpoint,
                        format!("странный ответ /api/version: {e}"),
                    )
                }
            },
            Err(_) => {
                return BackendStatus::unavailable(
                    "ollama",
                    endpoint,
                    "Ollama не отвечает. Установите её с ollama.com и запустите.".into(),
                )
            }
        };

        let models = self.installed_models().await.unwrap_or_default();
        // Ollama хранит теги как `name:tag`; пользователь в настройках может
        // написать `qwen3-vl:8b` или просто `qwen3-vl` — считаем оба вариантами.
        let has = |want: &str| {
            models
                .iter()
                .any(|m| m == want || m.split(':').next() == Some(want))
        };
        let vision_ready = has(&self.cfg.vision_model);
        let text_ready = has(&self.cfg.text_model);

        let detail = if vision_ready && text_ready {
            "Готово к работе".to_string()
        } else {
            let mut missing = Vec::new();
            if !vision_ready {
                missing.push(self.cfg.vision_model.clone());
            }
            if !text_ready && self.cfg.text_model != self.cfg.vision_model {
                missing.push(self.cfg.text_model.clone());
            }
            format!("Не скачаны модели: {}", missing.join(", "))
        };

        BackendStatus {
            kind: "ollama".into(),
            available: true,
            version: Some(version),
            endpoint,
            models,
            vision_model_ready: vision_ready,
            text_model_ready: text_ready,
            detail,
        }
    }

    fn body(&self, req: &ChatRequest, stream: bool) -> Value {
        let messages: Vec<Value> = req
            .messages
            .iter()
            .map(|m| {
                let mut obj = json!({ "role": m.role.as_str(), "content": m.content });
                if !m.images.is_empty() {
                    obj["images"] = json!(m.images);
                }
                obj
            })
            .collect();

        let mut body = json!({
            "model": req.model,
            "messages": messages,
            "stream": stream,
            "keep_alive": self.cfg.keep_alive,
            "options": {
                "temperature": req.temperature,
                "top_p": req.top_p,
                "num_predict": req.max_tokens,
            }
        });
        if req.json_mode {
            body["format"] = json!("json");
        }
        body
    }

    pub async fn chat(&self, req: &ChatRequest) -> AppResult<String> {
        let resp = self
            .http
            .post(self.url("/api/chat"))
            .json(&self.body(req, false))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Backend(format!("Ollama ответила {status}: {text}")));
        }

        let chunk: ChatChunk = resp.json().await?;
        if let Some(err) = chunk.error {
            return Err(AppError::Backend(err));
        }
        Ok(chunk.message.map(|m| m.content).unwrap_or_default())
    }

    pub async fn chat_stream(&self, req: &ChatRequest, sink: &TokenSink) -> AppResult<String> {
        let resp = self
            .http
            .post(self.url("/api/chat"))
            .json(&self.body(req, true))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Backend(format!("Ollama ответила {status}: {text}")));
        }

        let mut stream = resp.bytes_stream();
        let mut full = String::new();
        // Ollama отдаёт NDJSON, но TCP-чанк может разрезать строку пополам —
        // копим хвост до перевода строки.
        let mut buffer = String::new();

        while let Some(item) = stream.next().await {
            let bytes = item?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(idx) = buffer.find('\n') {
                let line: String = buffer.drain(..=idx).collect();
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let chunk: ChatChunk = serde_json::from_str(line)?;
                if let Some(err) = chunk.error {
                    return Err(AppError::Backend(err));
                }
                if let Some(msg) = chunk.message {
                    if !msg.content.is_empty() {
                        full.push_str(&msg.content);
                        // Получатель мог отвалиться (окно закрыли) — это не ошибка
                        // генерации, просто перестаём слать.
                        let _ = sink.send(msg.content);
                    }
                }
                if chunk.done {
                    return Ok(full);
                }
            }
        }
        Ok(full)
    }

    /// Скачивание модели с прогрессом. Сырые строки прогресса уходят в `sink`.
    pub async fn pull(&self, model: &str, sink: &TokenSink) -> AppResult<()> {
        let resp = self
            .http
            .post(self.url("/api/pull"))
            .json(&json!({ "model": model, "stream": true }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Backend(format!("Ollama ответила {status}: {text}")));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        while let Some(item) = stream.next().await {
            buffer.push_str(&String::from_utf8_lossy(&item?));
            while let Some(idx) = buffer.find('\n') {
                let line: String = buffer.drain(..=idx).collect();
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let v: Value = serde_json::from_str(line)?;
                if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
                    return Err(AppError::Backend(err.to_string()));
                }
                let _ = sink.send(line.to_string());
            }
        }
        Ok(())
    }
}
