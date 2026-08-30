use serde::{Serialize, Serializer};

/// Единый тип ошибки приложения. Всё, что уходит во фронтенд, сериализуется
/// в строку — Tauri требует `Serialize` от ошибки команды.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("не удалось связаться с локальной моделью: {0}")]
    Backend(String),

    #[error("бэкенд недоступен: {0}")]
    BackendUnavailable(String),

    #[error("модель вернула ответ, который не удалось разобрать: {0}")]
    BadModelOutput(String),

    #[error("ошибка работы с изображением: {0}")]
    Image(String),

    #[error("ошибка ввода-вывода: {0}")]
    Io(#[from] std::io::Error),

    #[error("ошибка конфигурации: {0}")]
    Config(String),

    #[error("{0}")]
    Other(String),
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_connect() {
            AppError::BackendUnavailable(e.to_string())
        } else {
            AppError::Backend(e.to_string())
        }
    }
}

impl From<image::ImageError> for AppError {
    fn from(e: image::ImageError) -> Self {
        AppError::Image(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::BadModelOutput(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
