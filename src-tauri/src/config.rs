use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};

/// Какой бэкенд используем для инференса.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    /// Автовыбор: если поднята Ollama — берём её, иначе встроенный llama-server.
    Auto,
    Ollama,
    LlamaCpp,
}

impl Default for BackendKind {
    fn default() -> Self {
        BackendKind::Auto
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OllamaConfig {
    pub base_url: String,
    /// Мультимодальная модель: распознаёт товар по фото.
    pub vision_model: String,
    /// Текстовая модель для копирайтинга. Может совпадать с vision_model —
    /// тогда Ollama не будет перезагружать веса между этапами.
    pub text_model: String,
    /// Сколько держать модель в VRAM после запроса (формат Ollama: "10m", "1h", "-1").
    pub keep_alive: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:11434".into(),
            // Подобрано под RTX 3080 10 ГБ: Q4-квантование 8B VLM занимает ~6.5 ГБ,
            // остаётся запас на KV-кэш и на проекцию изображения.
            vision_model: "qwen3-vl:8b".into(),
            text_model: "qwen3-vl:8b".into(),
            keep_alive: "15m".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlamaCppConfig {
    /// Путь до llama-server.exe. Пусто — ищем в `<ресурсы приложения>/llm/`.
    pub server_binary: Option<PathBuf>,
    /// GGUF с весами модели.
    pub model_path: Option<PathBuf>,
    /// GGUF с мультимодальной проекцией (mmproj-*.gguf) — без него нет зрения.
    pub mmproj_path: Option<PathBuf>,
    pub port: u16,
    /// Сколько слоёв выгружать на GPU. 999 = всё, что влезет.
    pub gpu_layers: u32,
    pub context_size: u32,
}

impl Default for LlamaCppConfig {
    fn default() -> Self {
        Self {
            server_binary: None,
            model_path: None,
            mmproj_path: None,
            port: 18434,
            gpu_layers: 999,
            context_size: 8192,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GenerationConfig {
    pub temperature: f32,
    pub top_p: f32,
    /// Верхняя граница длины описания в символах. Ресерч по Авито: оптимум
    /// 800–1500 знаков, дальше растёт «вода» и падает дочитываемость.
    pub target_chars_max: u32,
    pub target_chars_min: u32,
    /// Максимальная сторона фото перед отправкой в модель, px.
    pub image_max_side: u32,
    pub image_jpeg_quality: u8,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: 0.9,
            target_chars_max: 1500,
            target_chars_min: 800,
            image_max_side: 1024,
            image_jpeg_quality: 85,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub backend: BackendKind,
    pub ollama: OllamaConfig,
    pub llama_cpp: LlamaCppConfig,
    pub generation: GenerationConfig,
    /// Данные продавца подставляются в блок «условия сделки».
    pub seller: SellerProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SellerProfile {
    /// `private` — частное лицо, `shop` — магазин/ИП. Меняет тон описания.
    pub kind: String,
    pub city: String,
    pub delivery: String,
    pub pickup: String,
    pub bargain: bool,
}

impl Default for SellerProfile {
    fn default() -> Self {
        Self {
            kind: "private".into(),
            city: String::new(),
            delivery: "Авито Доставка".into(),
            pickup: String::new(),
            bargain: false,
        }
    }
}

fn config_path(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| AppError::Config(format!("не найден каталог конфигурации: {e}")))?;
    Ok(dir.join("config.json"))
}

pub fn load(app: &AppHandle) -> AppResult<AppConfig> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let raw = std::fs::read_to_string(&path)?;
    // Битый конфиг не должен блокировать запуск приложения.
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn save(app: &AppHandle, cfg: &AppConfig) -> AppResult<()> {
    let path = config_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(cfg)?)?;
    Ok(())
}
