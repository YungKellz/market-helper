use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc::unbounded_channel;

use crate::config::{self, AppConfig};
use crate::error::{AppError, AppResult};
use crate::imaging;
use crate::llm::types::{BackendStatus, TokenSink};
use crate::llm::LlmService;
use crate::pipeline::schema::{
    GenerateOptions, ListingDraft, ListingResult, ProductFacts, UserAttributes,
};
use crate::pipeline;

/// Событие с очередным куском текста от модели.
pub const EVENT_TOKEN: &str = "generation:token";
/// Событие с прогрессом скачивания модели.
pub const EVENT_PULL: &str = "model:pull";

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub llm: LlmService,
    /// Подготовленные фото живут в Rust: гонять многомегабайтный base64
    /// через IPC на каждый запрос незачем.
    pub photos: Mutex<HashMap<String, StoredPhoto>>,
}

pub struct StoredPhoto {
    pub b64: String,
}

/// То, что уходит во фронтенд после загрузки фото.
#[derive(Debug, Clone, Serialize)]
pub struct PhotoInfo {
    pub id: String,
    pub file_name: String,
    pub preview: String,
    pub width: u32,
    pub height: u32,
}

impl AppState {
    fn config(&self) -> AppConfig {
        self.config.lock().expect("состояние конфигурации отравлено").clone()
    }

    fn images_for(&self, ids: &[String]) -> Vec<String> {
        let photos = self.photos.lock().expect("хранилище фото отравлено");
        ids.iter()
            .filter_map(|id| photos.get(id).map(|p| p.b64.clone()))
            .collect()
    }
}

/// Пробрасывает токены из бэкенда в окно приложения.
fn spawn_token_forwarder(app: AppHandle, event: &'static str) -> TokenSink {
    let (tx, mut rx) = unbounded_channel::<String>();
    tauri::async_runtime::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            let _ = app.emit(event, chunk);
        }
    });
    tx
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> AppConfig {
    state.config()
}

#[tauri::command]
pub fn save_config(app: AppHandle, state: State<'_, AppState>, config: AppConfig) -> AppResult<()> {
    config::save(&app, &config)?;
    *state.config.lock().expect("состояние конфигурации отравлено") = config;
    Ok(())
}

#[tauri::command]
pub async fn backend_status(state: State<'_, AppState>) -> AppResult<Vec<BackendStatus>> {
    let cfg = state.config();
    Ok(state.llm.statuses(&cfg).await)
}

/// Скачивание модели через Ollama с прогрессом в событии `model:pull`.
#[tauri::command]
pub async fn pull_model(
    app: AppHandle,
    state: State<'_, AppState>,
    model: String,
) -> AppResult<()> {
    let cfg = state.config();
    let sink = spawn_token_forwarder(app, EVENT_PULL);
    state.llm.ollama(&cfg).pull(&model, &sink).await
}

#[tauri::command]
pub fn add_photos(state: State<'_, AppState>, paths: Vec<String>) -> AppResult<Vec<PhotoInfo>> {
    let cfg = state.config();
    let mut added = Vec::with_capacity(paths.len());

    for path in paths {
        let path = PathBuf::from(path);
        let prepared = imaging::prepare_from_path(
            &path,
            cfg.generation.image_max_side,
            cfg.generation.image_jpeg_quality,
        )?;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "фото".to_string());
        added.push(store(&state, prepared, file_name));
    }
    Ok(added)
}

/// Путь для drag&drop из проводника в окно: браузерный File API отдаёт байты,
/// а не путь.
#[tauri::command]
pub fn add_photo_bytes(
    state: State<'_, AppState>,
    file_name: String,
    bytes: Vec<u8>,
) -> AppResult<PhotoInfo> {
    let cfg = state.config();
    let prepared = imaging::prepare_from_bytes(
        &bytes,
        cfg.generation.image_max_side,
        cfg.generation.image_jpeg_quality,
    )?;
    Ok(store(&state, prepared, file_name))
}

fn store(state: &State<'_, AppState>, prepared: imaging::PreparedImage, file_name: String) -> PhotoInfo {
    let id = uuid::Uuid::new_v4().to_string();
    let info = PhotoInfo {
        id: id.clone(),
        file_name,
        preview: prepared.preview_data_url,
        width: prepared.width,
        height: prepared.height,
    };
    state
        .photos
        .lock()
        .expect("хранилище фото отравлено")
        .insert(id, StoredPhoto { b64: prepared.b64 });
    info
}

#[tauri::command]
pub fn remove_photo(state: State<'_, AppState>, id: String) {
    state.photos.lock().expect("хранилище фото отравлено").remove(&id);
}

#[tauri::command]
pub async fn analyze_photos(
    state: State<'_, AppState>,
    photo_ids: Vec<String>,
    hint: String,
) -> AppResult<ProductFacts> {
    let cfg = state.config();
    let images = state.images_for(&photo_ids);
    if images.is_empty() {
        return Err(AppError::Other("сначала добавьте хотя бы одно фото".into()));
    }
    let backend = state.llm.resolve(&cfg).await?;
    pipeline::analyze_photos(&backend, &cfg, images, &hint).await
}

#[tauri::command]
pub async fn generate_listing(
    app: AppHandle,
    state: State<'_, AppState>,
    facts: ProductFacts,
    attributes: UserAttributes,
    options: GenerateOptions,
) -> AppResult<ListingResult> {
    let cfg = state.config();
    let backend = state.llm.resolve(&cfg).await?;
    let sink = spawn_token_forwarder(app, EVENT_TOKEN);
    pipeline::generate_listing(&backend, &cfg, &facts, &attributes, &options, &sink).await
}

#[tauri::command]
pub async fn refine_listing(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: ListingDraft,
    instruction: String,
    options: GenerateOptions,
) -> AppResult<ListingResult> {
    if instruction.trim().is_empty() {
        return Err(AppError::Other("опишите, что нужно исправить".into()));
    }
    let cfg = state.config();
    let backend = state.llm.resolve(&cfg).await?;
    let sink = spawn_token_forwarder(app, EVENT_TOKEN);
    pipeline::refine_listing(&backend, &cfg, &draft, &instruction, &options, &sink).await
}

/// Повторная проверка текста после ручной правки в интерфейсе.
#[tauri::command]
pub fn lint_listing(state: State<'_, AppState>, draft: ListingDraft) -> ListingResult {
    let cfg = state.config();
    ListingResult {
        title_chars: draft.title.chars().count(),
        description_chars: draft.description.chars().count(),
        issues: pipeline::lint::check(&draft, &cfg.generation),
        backend: String::new(),
        draft,
    }
}
