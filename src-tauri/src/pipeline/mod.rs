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
    if raw.trim().is_empty() {
        return Err(AppError::BadModelOutput(
            "модель вернула пустой ответ. Обычно это значит, что лимит токенов              закончился раньше, чем она добралась до ответа — попробуйте ещё раз              или возьмите модель полегче в настройках"
                .into(),
        ));
    }
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
    // С отключёнными размышлениями ответ по схеме укладывается в ~200 токенов,
    // остальное — запас на многословную модель.
    .max_tokens(2048)
    .json()
    .no_thinking();

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
    .max_tokens(2600)
    .json()
    .no_thinking();

    stream_and_parse(backend, cfg, req, sink).await
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
    // Переделка дороже первой генерации: модель держит в контексте весь
    // прежний текст и обязана выдать его заново целиком.
    .max_tokens(3072)
    .json()
    .no_thinking();

    stream_and_parse(backend, cfg, req, sink).await
}

/// Локальная 8B изредка обрывает JSON на полуслове, упёршись в лимит токенов.
/// Одна повторная попытка с увеличенным лимитом дешевле, чем ошибка в лицо
/// пользователю; наружу отдаём исходную ошибку, она информативнее.
async fn stream_and_parse(
    backend: &Backend,
    cfg: &AppConfig,
    req: ChatRequest,
    sink: &TokenSink,
) -> AppResult<ListingResult> {
    let raw = backend.chat_stream(&req, sink).await?;
    let first = match finish(raw, cfg, backend.kind()) {
        Ok(result) => return Ok(result),
        Err(e) => e,
    };

    let retry = req.clone().max_tokens(req.max_tokens + 1024);
    let raw = backend.chat_stream(&retry, sink).await?;
    finish(raw, cfg, backend.kind()).map_err(|_| first)
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

    #[test]
    fn empty_output_explains_itself() {
        let err = extract_json("   ").unwrap_err().to_string();
        assert!(err.contains("пустой ответ"), "невнятная ошибка: {err}");
    }

    /// Полный путь «файл на диске → факты о товаре» против живой Ollama.
    /// Путь к картинке задаётся через MARKET_HELPER_TEST_IMAGE; без него
    /// и без запущенной Ollama тест молча проходит.
    #[tokio::test]
    async fn live_vision_pipeline_produces_facts() {
        let Ok(image_path) = std::env::var("MARKET_HELPER_TEST_IMAGE") else {
            eprintln!("MARKET_HELPER_TEST_IMAGE не задан — живой тест пропущен");
            return;
        };

        let cfg = AppConfig::default();
        let service = crate::llm::LlmService::new(None);
        if !service.ollama(&cfg).status().await.available {
            eprintln!("Ollama не запущена — живой тест пропущен");
            return;
        }

        let prepared = crate::imaging::prepare_from_path(
            std::path::Path::new(&image_path),
            cfg.generation.image_max_side,
            cfg.generation.image_jpeg_quality,
        )
        .expect("не удалось подготовить изображение");

        let backend = service.resolve(&cfg).await.expect("бэкенд недоступен");
        let facts = analyze_photos(&backend, &cfg, vec![prepared.b64], "")
            .await
            .expect("распознавание провалилось");

        eprintln!("распознано: {facts:?}");
        assert!(
            !facts.product_type.trim().is_empty() || !facts.category.trim().is_empty(),
            "модель не заполнила ни тип товара, ни категорию"
        );
    }

    /// Копирайтинг идёт по отдельному, потоковому пути — а именно там ответ
    /// и уезжал в поле thinking. Проверяем его живьём отдельно.
    #[tokio::test]
    async fn live_copy_stage_produces_listing() {
        let cfg = AppConfig::default();
        let service = crate::llm::LlmService::new(None);
        if !service.ollama(&cfg).status().await.available {
            eprintln!("Ollama не запущена — живой тест пропущен");
            return;
        }

        let attrs = UserAttributes {
            title_hint: "Самокат детский Novatrack".into(),
            condition: "хорошее, катались один сезон".into(),
            price: "4500".into(),
            defects: "потёртость на деке".into(),
            ..Default::default()
        };

        let backend = service.resolve(&cfg).await.expect("бэкенд недоступен");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let collector = tokio::spawn(async move {
            let mut seen = 0usize;
            while let Some(chunk) = rx.recv().await {
                seen += chunk.len();
            }
            seen
        });

        let result = generate_listing(
            &backend,
            &cfg,
            &ProductFacts::default(),
            &attrs,
            &GenerateOptions::default(),
            &tx,
        )
        .await
        .expect("генерация провалилась");
        drop(tx);

        let streamed = collector.await.unwrap();
        eprintln!(
            "заголовок: {}
символов в описании: {}
стримом пришло: {streamed}",
            result.draft.title, result.description_chars
        );
        assert!(!result.draft.title.trim().is_empty(), "пустой заголовок");
        assert!(result.description_chars > 100, "описание подозрительно короткое");
        assert!(streamed > 0, "в поток не пришло ни одного символа");
    }

    /// Переделка готового текста — тот же потоковый путь, но с другим промптом.
    #[tokio::test]
    async fn live_refine_rewrites_listing() {
        let cfg = AppConfig::default();
        let service = crate::llm::LlmService::new(None);
        if !service.ollama(&cfg).status().await.available {
            eprintln!("Ollama не запущена — живой тест пропущен");
            return;
        }

        let current = ListingDraft {
            title: "Спрей Simple Line анти пыль 500 мл дозатор".into(),
            hook: "Спрей-дозатор 500 мл, белый корпус.".into(),
            description: "Спрей-дозатор 500 мл, белый корпус с надписью Simple Line.                           Антипыльное средство для уборки, удаляет пыль и загрязнения.                           Хорошее состояние: царапины на корпусе, но функционал работает                           безупречно. Доставка через Авито Доставку."
                .into(),
            tags: vec!["спрей анти пыль".into()],
        };

        let backend = service.resolve(&cfg).await.expect("бэкенд недоступен");
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        let result = refine_listing(
            &backend,
            &cfg,
            &current,
            "Смени тон на более деловой",
            &GenerateOptions::default(),
            &tx,
        )
        .await
        .expect("переделка провалилась");

        eprintln!(
            "заголовок: {}
описание ({} символов): {}",
            result.draft.title, result.description_chars, result.draft.description
        );
        assert!(!result.draft.description.trim().is_empty(), "пустое описание");
        assert_ne!(
            result.draft.description, current.description,
            "текст не изменился после переделки"
        );
    }
}
