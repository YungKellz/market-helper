use std::io::Cursor;
use std::path::Path;

use base64::Engine;
use image::{DynamicImage, ImageFormat};

use crate::error::{AppError, AppResult};

/// Фото, приведённое к виду, который понимает модель.
pub struct PreparedImage {
    /// JPEG в base64 — то, что уходит в Ollama/llama.cpp.
    pub b64: String,
    /// Уменьшенная копия для превью в интерфейсе (data URL).
    pub preview_data_url: String,
    pub width: u32,
    pub height: u32,
}

/// EXIF-ориентация: снимки с телефона почти всегда лежат «боком», а модели
/// это заметно мешает — читает текст на этикетке хуже.
fn exif_orientation(bytes: &[u8]) -> Option<u32> {
    let mut cursor = Cursor::new(bytes);
    let exif = exif::Reader::new()
        .read_from_container(&mut cursor)
        .ok()?;
    exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?
        .value
        .get_uint(0)
}

fn apply_orientation(img: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> AppResult<Vec<u8>> {
    // JPEG не умеет альфу — схлопываем в RGB8, иначе кодировщик вернёт ошибку.
    let rgb = img.to_rgb8();
    let mut buf = Vec::new();
    let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(Cursor::new(&mut buf), quality);
    encoder.encode(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        image::ExtendedColorType::Rgb8,
    )?;
    Ok(buf)
}

pub fn prepare_from_bytes(bytes: &[u8], max_side: u32, quality: u8) -> AppResult<PreparedImage> {
    let orientation = exif_orientation(bytes).unwrap_or(1);
    let decoded = image::load_from_memory(bytes)?;
    let img = apply_orientation(decoded, orientation);

    let (w, h) = (img.width(), img.height());
    // Апскейл бессмысленен: он не добавляет деталей, но раздувает число
    // визуальных токенов и время генерации.
    let model_img = if w.max(h) > max_side {
        img.resize(max_side, max_side, image::imageops::FilterType::CatmullRom)
    } else {
        img.clone()
    };

    let jpeg = encode_jpeg(&model_img, quality)?;
    let preview = img.resize(384, 384, image::imageops::FilterType::Triangle);
    let preview_jpeg = encode_jpeg(&preview, 78)?;

    let engine = base64::engine::general_purpose::STANDARD;
    Ok(PreparedImage {
        b64: engine.encode(&jpeg),
        preview_data_url: format!("data:image/jpeg;base64,{}", engine.encode(&preview_jpeg)),
        width: model_img.width(),
        height: model_img.height(),
    })
}

pub fn prepare_from_path(path: &Path, max_side: u32, quality: u8) -> AppResult<PreparedImage> {
    // Авито принимает jpg/jpeg/png/gif — но пользователю удобнее, когда
    // приложение съедает всё, что декодирует `image`, и само нормализует.
    if ImageFormat::from_path(path).is_err() {
        return Err(AppError::Image(format!(
            "неподдерживаемый формат файла: {}",
            path.display()
        )));
    }
    let bytes = std::fs::read(path)?;
    prepare_from_bytes(&bytes, max_side, quality)
}
