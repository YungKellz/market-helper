pub mod lint;
pub mod prompts;
pub mod schema;

use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::llm::types::{ChatMessage, ChatRequest, TokenSink};
use crate::llm::Backend;
use crate::pipeline::schema::{
    GenerateOptions, ListingDraft, ListingResult, ProductFacts, UserAttributes,
};

/// Локальные модели любят обернуть JSON в ```json-блок или добавить фразу
/// «Вот результат:». Вырезаем первый сбалансированный объект.
fn extract_json(raw: &str) -> AppResult<&str> {
    let bytes = raw.as_bytes();
    let start = raw
        .find('{')
        .ok_or_else(|| AppError::BadModelOutput(truncate(raw)))?;

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for i in start..bytes.len() {
        let c = bytes[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            continue;
        }
        match c {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(&raw[start..=i]);
                }
            }
            _ => {}
        }
    }
    Err(AppError::BadModelOutput(truncate(raw)))
}

fn truncate(s: &str) -> String {
    let cut: String = s.chars().take(300).collect();
    cut
}

fn parse_json<T: serde::de::DeserializeOwned>(raw: &str) -> AppResult<T> {
    Ok(serde_json::from_str(extract_json(raw)?)?)
}

/// Этап 1: что за товар на фото.
pub async fn analyze_photos(
    backend: &Backend,
    cfg: &AppConfig,
    images: Vec<String>,
    hint: &str,
) -> AppResult<ProductFacts> {
    if images.is_empty() {
        return Err(AppError::Other("не загружено ни одного фото".into()));
    }

    let mut user = String::from("Определи товар по фотографиям и верни JSON.");
    if !hint.trim().is_empty() {
        user.push_str(&format!(
            "\n\nПодсказка от пользователя (учитывай её как достоверную): {}",
            hint.trim()
        ));
    }

    let req = ChatRequest::new(
        &cfg.ollama.vision_model,
        vec![
            ChatMessage::system(prompts::VISION_SYSTEM),
            ChatMessage::user_with_images(user, images),
        ],
    )
    // Распознавание — задача на точность, а не на творчество.
    .sampling(0.15, 0.9)
    .max_tokens(900)
    .json();

    let raw = backend.chat(&req).await?;
    let mut facts: ProductFacts = parse_json(&raw)?;
    facts.confidence = facts.confidence.clamp(0.0, 1.0);
    Ok(facts)
}

/// Этап 2: продающий текст. Токены уходят в `sink` по мере генерации, чтобы
/// пользователь видел прогресс — на локальной 8B это десятки секунд.
pub async fn generate_listing(
    backend: &Backend,
    cfg: &AppConfig,
    facts: &ProductFacts,
    attrs: &UserAttributes,
    opts: &GenerateOptions,
    sink: &TokenSink,
) -> AppResult<ListingResult> {
    let req = ChatRequest::new(
        &cfg.ollama.text_model,
        vec![
            ChatMessage::system(prompts::copy_system(cfg, opts)),
            ChatMessage::user(prompts::copy_user(facts, attrs, opts, &cfg.seller)),
        ],
    )
    .sampling(cfg.generation.temperature, cfg.generation.top_p)
    // ~1500 символов кириллицы это ориентировочно 700–900 токенов; запас нужен
    // на заголовок, теги и JSON-обвязку.
    .max_tokens(1800)
    .json();

    let raw = backend.chat_stream(&req, sink).await?;
    finish(raw, cfg, backend.kind())
}

/// Точечная правка готового текста по инструкции пользователя.
pub async fn refine_listing(
    backend: &Backend,
    cfg: &AppConfig,
    current: &ListingDraft,
    instruction: &str,
    opts: &GenerateOptions,
    sink: &TokenSink,
) -> AppResult<ListingResult> {
    let current_text = format!("Заголовок: {}\n\n{}", current.title, current.description);
    let req = ChatRequest::new(
        &cfg.ollama.text_model,
        vec![
            ChatMessage::system(prompts::copy_system(cfg, opts)),
            ChatMessage::user(prompts::refine_user(&current_text, instruction)),
        ],
    )
    .sampling(cfg.generation.temperature, cfg.generation.top_p)
    .max_tokens(1800)
    .json();

    let raw = backend.chat_stream(&req, sink).await?;
    finish(raw, cfg, backend.kind())
}

fn finish(raw: String, cfg: &AppConfig, backend_kind: &str) -> AppResult<ListingResult> {
    let mut draft: ListingDraft = parse_json(&raw)?;
    draft.title = draft.title.trim().to_string();
    draft.description = draft.description.trim().to_string();
    draft.hook = draft.hook.trim().to_string();

    // Модель нередко возвращает пустой hook, хотя текст в описании уже есть.
    if draft.hook.is_empty() && !draft.description.is_empty() {
        draft.hook = draft.description.chars().take(200).collect();
    }
    draft.tags.retain(|t| !t.trim().is_empty());

    let issues = lint::check(&draft, &cfg.generation);
    Ok(ListingResult {
        title_chars: draft.title.chars().count(),
        description_chars: draft.description.chars().count(),
        issues,
        backend: backend_kind.to_string(),
        draft,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_json_from_markdown_fence() {
        let raw = "Вот результат:\n```json\n{\"title\": \"Стул\"}\n```";
        assert_eq!(extract_json(raw).unwrap(), "{\"title\": \"Стул\"}");
    }

    #[test]
    fn extracts_nested_object_without_eating_braces_in_strings() {
        let raw = r#"{"a": {"b": "}"}, "c": 1} лишний хвост"#;
        assert_eq!(extract_json(raw).unwrap(), r#"{"a": {"b": "}"}, "c": 1}"#);
    }

    #[test]
    fn fails_on_output_without_json() {
        assert!(extract_json("модель отказалась отвечать").is_err());
    }
}
