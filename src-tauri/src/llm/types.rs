use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    /// JPEG-кадры в base64 (без префикса `data:`).
    pub images: Vec<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into(), images: Vec::new() }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into(), images: Vec::new() }
    }

    pub fn user_with_images(content: impl Into<String>, images: Vec<String>) -> Self {
        Self { role: Role::User, content: content.into(), images }
    }
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    pub top_p: f32,
    pub max_tokens: u32,
    /// Просим бэкенд гарантировать валидный JSON на выходе.
    pub json_mode: bool,
    /// Управление «размышлениями» reasoning-моделей. `None` — не трогаем.
    pub think: Option<bool>,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            temperature: 0.7,
            top_p: 0.9,
            max_tokens: 1600,
            json_mode: false,
            think: None,
        }
    }

    pub fn json(mut self) -> Self {
        self.json_mode = true;
        self
    }

    pub fn sampling(mut self, temperature: f32, top_p: f32) -> Self {
        self.temperature = temperature;
        self.top_p = top_p;
        self
    }

    pub fn max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n;
        self
    }

    /// Reasoning-модель без этого флага скармливает весь лимит токенов
    /// размышлениям и до самого ответа не доходит. Нашим задачам — извлечь
    /// факты по схеме и написать текст по шаблону — цепочка рассуждений
    /// не нужна.
    pub fn no_thinking(mut self) -> Self {
        self.think = Some(false);
        self
    }
}

/// Куда бэкенд шлёт токены по мере генерации.
pub type TokenSink = UnboundedSender<String>;

#[derive(Debug, Clone, Serialize)]
pub struct BackendStatus {
    /// `ollama` | `llama_cpp` | `none`
    pub kind: String,
    pub available: bool,
    pub version: Option<String>,
    pub endpoint: String,
    /// Установленные модели (для Ollama).
    pub models: Vec<String>,
    /// Есть ли среди установленных та, что выбрана в настройках.
    pub vision_model_ready: bool,
    pub text_model_ready: bool,
    pub detail: String,
}

impl BackendStatus {
    pub fn unavailable(kind: &str, endpoint: String, detail: String) -> Self {
        Self {
            kind: kind.to_string(),
            available: false,
            version: None,
            endpoint,
            models: Vec::new(),
            vision_model_ready: false,
            text_model_ready: false,
            detail,
        }
    }
}
