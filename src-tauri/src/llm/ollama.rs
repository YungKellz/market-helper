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
    /// Ollama 0.33 с qwen3-vl складывает сюда весь ответ, а `content`
    /// оставляет пустым. Читаем оба поля и берём то, где что-то есть.
    #[serde(default)]
    thinking: String,
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
        if let Some(think) = req.think {
            body["think"] = json!(think);
        }
        body
    }

    /// Отправка с одной повторной попыткой: модели без поддержки размышлений
    /// отвергают параметр `think` целиком, и это не повод падать.
    async fn send(&self, req: &ChatRequest, stream: bool) -> AppResult<reqwest::Response> {
        let mut body = self.body(req, stream);
        let resp = self
            .http
            .post(self.url("/api/chat"))
            .json(&body)
            .send()
            .await?;

        if resp.status() != reqwest::StatusCode::BAD_REQUEST || body.get("think").is_none() {
            return Ok(resp);
        }

        let text = resp.text().await.unwrap_or_default();
        if !text.contains("think") {
            return Err(AppError::Backend(format!("Ollama ответила 400: {text}")));
        }
        if let Some(obj) = body.as_object_mut() {
            obj.remove("think");
        }
        Ok(self
            .http
            .post(self.url("/api/chat"))
            .json(&body)
            .send()
            .await?)
    }

    pub async fn chat(&self, req: &ChatRequest) -> AppResult<String> {
        let resp = self.send(req, false).await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Backend(format!("Ollama ответила {status}: {text}")));
        }

        let chunk: ChatChunk = resp.json().await?;
        if let Some(err) = chunk.error {
            return Err(AppError::Backend(err));
        }
        Ok(chunk
            .message
            .map(|m| if m.content.is_empty() { m.thinking } else { m.content })
            .unwrap_or_default())
    }

    pub async fn chat_stream(&self, req: &ChatRequest, sink: &TokenSink) -> AppResult<String> {
        let resp = self.send(req, true).await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Backend(format!("Ollama ответила {status}: {text}")));
        }

        let mut stream = resp.bytes_stream();
        // Копим content и thinking раздельно: смешивать их нельзя, иначе
        // разбор JSON наткнётся на обрывок рассуждений вместо ответа.
        let mut content = String::new();
        let mut thinking = String::new();
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
                    // В превью шлём всё подряд: пользователю важно видеть, что
                    // модель работает, а не какое это поле протокола.
                    // Получатель мог отвалиться (окно закрыли) — это не ошибка
                    // генерации, просто перестаём слать.
                    if !msg.content.is_empty() {
                        content.push_str(&msg.content);
                        let _ = sink.send(msg.content);
                    }
                    if !msg.thinking.is_empty() {
                        thinking.push_str(&msg.thinking);
                        let _ = sink.send(msg.thinking);
                    }
                }
                if chunk.done {
                    return Ok(if content.is_empty() { thinking } else { content });
                }
            }
        }
        Ok(if content.is_empty() { thinking } else { content })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::types::ChatMessage;

    fn backend() -> OllamaBackend {
        OllamaBackend::new(OllamaConfig::default())
    }

    /// Живой прогон против запущенной Ollama. Без неё тест молча проходит,
    /// чтобы не ломать сборку на машине без движка.
    #[tokio::test]
    async fn live_chat_returns_parseable_json() {
        let backend = backend();
        if !backend.status().await.available {
            eprintln!("Ollama не запущена — живой тест пропущен");
            return;
        }

        let model = OllamaConfig::default().text_model;
        if !backend
            .installed_models()
            .await
            .unwrap_or_default()
            .iter()
            .any(|m| *m == model)
        {
            eprintln!("модель {model} не скачана — живой тест пропущен");
            return;
        }

        let req = ChatRequest::new(
            &model,
            vec![
                ChatMessage::system("Отвечай только валидным JSON без пояснений."),
                ChatMessage::user("Верни {\"product_type\": \"стул\"}"),
            ],
        )
        .sampling(0.1, 0.9)
        .max_tokens(2048)
        .json()
        .no_thinking();

        let raw = backend.chat(&req).await.expect("запрос к Ollama провалился");

        // Ровно та регрессия, из-за которой распознавание падало: ответ уезжал
        // в поле thinking, а мы читали только content и получали пустоту.
        assert!(!raw.trim().is_empty(), "модель вернула пустой ответ");
        let parsed: serde_json::Value =
            serde_json::from_str(raw.trim()).unwrap_or_else(|e| panic!("не JSON: {e}\n{raw}"));
        assert!(parsed.is_object(), "ожидался JSON-объект, получили {parsed}");
    }
}
