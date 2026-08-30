use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

use crate::config::GenerationConfig;
use crate::pipeline::schema::ListingDraft;

/// Насколько серьёзна находка. `error` — объявление, скорее всего, снимут
/// с публикации; `warning` — текст опубликуют, но он хуже продаёт.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    pub severity: Severity,
    /// Где найдено: `title` | `description` | `hook` | `tags`.
    pub field: String,
    pub message: String,
    /// Фрагмент, к которому относится замечание.
    pub excerpt: Option<String>,
}

impl Issue {
    fn error(field: &str, message: impl Into<String>, excerpt: Option<String>) -> Self {
        Self { severity: Severity::Error, field: field.into(), message: message.into(), excerpt }
    }

    fn warning(field: &str, message: impl Into<String>, excerpt: Option<String>) -> Self {
        Self { severity: Severity::Warning, field: field.into(), message: message.into(), excerpt }
    }
}

static PHONE: Lazy<Regex> = Lazy::new(|| {
    // +7 (999) 123-45-67, 89991234567, 999 123 45 67 — с любыми разделителями.
    Regex::new(r"(?:\+7|\b8|\b7)[\s\-().]*\d{3}[\s\-().]*\d{3}[\s\-().]*\d{2}[\s\-().]*\d{2}\b")
        .unwrap()
});

static EMAIL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[\w.+-]+@[\w-]+\.[a-zA-Zа-яА-Я]{2,}").unwrap());

static URL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:https?://|www\.)\S+|\b[\w-]+\.(?:ru|com|net|org|рф|su|io|me|shop|store)\b")
        .unwrap()
});

static MESSENGER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(?:whats\s?app|вотс\s?ап|ватс\s?ап|viber|вайбер|telegram|телеграм|телега|\bтг\b|@[A-Za-z][\w_]{3,})")
        .unwrap()
});

static OTHER_PLATFORMS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(?:ozon|озон|wildberries|вайлдберриз|вб|юла|yula|яндекс\s?маркет|aliexpress|алиэкспресс)\b")
        .unwrap()
});

static SUPERLATIVES: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:самый\s+(?:дешёв|дешев|низк|лучш|выгодн)|самая\s+(?:низк|лучш|выгодн)|самые\s+(?:низк|лучш|выгодн)|лучшая\s+цена|лучшее\s+предложение|дешевле\s+(?:не\s+найдёте|нигде)|ниже\s+рынка\s+гарантированно|№\s?1\s+на\s+рынке)")
        .unwrap()
});

static PRICE_IN_TITLE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:\d[\d\s.,]{2,}\s*(?:₽|руб|р\.|тыс|k\b)|цена\s*[:\-]?\s*\d)").unwrap()
});

static BANG_RUN: Lazy<Regex> = Lazy::new(|| Regex::new(r"!{2,}").unwrap());

/// Слова целиком в верхнем регистре длиной от 4 символов. Аббревиатуры вроде
/// «USB» или «LED» короче и под правило не попадают.
static SHOUTING: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b[А-ЯЁA-Z]{4,}\b").unwrap());

fn excerpt_of(re: &Regex, text: &str) -> Option<String> {
    re.find(text).map(|m| m.as_str().trim().to_string())
}

/// Проверки, которые нельзя доверять модели: она уверенно «забывает» правила
/// в длинном тексте, а цена ошибки — снятое с публикации объявление.
pub fn check(draft: &ListingDraft, cfg: &GenerationConfig) -> Vec<Issue> {
    let mut issues = Vec::new();
    let title = draft.title.trim();
    let desc = draft.description.trim();

    let contact_rules: [(&Regex, &str); 4] = [
        (&PHONE, "похоже на номер телефона — Авито запрещает контакты в описании"),
        (&EMAIL, "похоже на e-mail — Авито запрещает контакты в описании"),
        (&URL, "похоже на ссылку или адрес сайта — Авито запрещает ссылки в описании"),
        (&MESSENGER, "упоминание мессенджера или ника — Авито запрещает такие контакты"),
    ];

    for (field, text) in [("title", title), ("description", desc)] {
        for (re, message) in contact_rules {
            if let Some(found) = excerpt_of(re, text) {
                issues.push(Issue::error(field, message, Some(found)));
            }
        }
        if let Some(found) = excerpt_of(&OTHER_PLATFORMS, text) {
            issues.push(Issue::error(
                field,
                "упоминание другой торговой площадки — модерация Авито это снимает",
                Some(found),
            ));
        }
        if let Some(found) = excerpt_of(&SUPERLATIVES, text) {
            issues.push(Issue::warning(
                field,
                "необоснованная превосходная степень: снижает доверие и попадает под правила о недостоверной рекламе",
                Some(found),
            ));
        }
        if let Some(found) = excerpt_of(&SHOUTING, text) {
            issues.push(Issue::warning(
                field,
                "слово целиком капсом — Авито считает это привлечением внимания",
                Some(found),
            ));
        }
        if let Some(found) = excerpt_of(&BANG_RUN, text) {
            issues.push(Issue::warning(field, "несколько восклицательных знаков подряд", Some(found)));
        }
    }

    let title_chars = title.chars().count();
    if title_chars == 0 {
        issues.push(Issue::error("title", "заголовок пустой", None));
    } else if title_chars > 100 {
        issues.push(Issue::error(
            "title",
            format!("заголовок длиннее 100 символов ({title_chars}) — Авито его обрежет"),
            None,
        ));
    }
    if let Some(found) = excerpt_of(&PRICE_IN_TITLE, title) {
        issues.push(Issue::error(
            "title",
            "цена в заголовке запрещена — для неё есть отдельное поле",
            Some(found),
        ));
    }

    let desc_chars = desc.chars().count();
    if desc_chars == 0 {
        issues.push(Issue::error("description", "описание пустое", None));
    } else if desc_chars < cfg.target_chars_min as usize {
        issues.push(Issue::warning(
            "description",
            format!(
                "описание короче рекомендованных {} символов ({desc_chars}) — не хватает деталей для доверия",
                cfg.target_chars_min
            ),
            None,
        ));
    } else if desc_chars > cfg.target_chars_max as usize {
        issues.push(Issue::warning(
            "description",
            format!(
                "описание длиннее рекомендованных {} символов ({desc_chars}) — такой текст редко дочитывают",
                cfg.target_chars_max
            ),
            None,
        ));
    }

    // Первые ~200 символов — единственное, что видно в поисковой выдаче.
    let hook = draft.hook.trim();
    if !hook.is_empty() {
        let hook_chars = hook.chars().count();
        if hook_chars > 200 {
            issues.push(Issue::warning(
                "hook",
                format!("хук длиннее 200 символов ({hook_chars}) — в выдаче он оборвётся"),
                None,
            ));
        }
        let hook_prefix: String = hook.chars().take(40).collect();
        if !desc.starts_with(&hook_prefix) {
            issues.push(Issue::warning(
                "hook",
                "хук не совпадает с началом описания — в выдаче покупатель увидит другой текст",
                None,
            ));
        }
    }

    if draft.tags.len() > 12 {
        issues.push(Issue::warning(
            "tags",
            format!("{} ключевых фраз — переизбыток тегов ведёт к пессимизации", draft.tags.len()),
            None,
        ));
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> GenerationConfig {
        GenerationConfig::default()
    }

    fn draft(title: &str, description: &str) -> ListingDraft {
        ListingDraft {
            title: title.into(),
            hook: String::new(),
            description: description.into(),
            tags: Vec::new(),
        }
    }

    fn has_error(issues: &[Issue], needle: &str) -> bool {
        issues
            .iter()
            .any(|i| i.severity == Severity::Error && i.message.contains(needle))
    }

    #[test]
    fn catches_phone_in_any_common_format() {
        for phone in ["+7 (999) 123-45-67", "89991234567", "8 999 123 45 67"] {
            let d = draft("Стул", &format!("Хороший стул. Звоните {phone}."));
            assert!(
                has_error(&check(&d, &cfg()), "телефон"),
                "не найден телефон в {phone}"
            );
        }
    }

    #[test]
    fn catches_links_and_messengers() {
        let d = draft("Стул", "Каталог на example.ru, пишите в телеграм.");
        let issues = check(&d, &cfg());
        assert!(has_error(&issues, "ссылку"));
        assert!(has_error(&issues, "мессенджера"));
    }

    #[test]
    fn catches_price_in_title() {
        let d = draft("Диван угловой 25 000 руб", "Описание.");
        assert!(has_error(&check(&d, &cfg()), "цена в заголовке"));
    }

    #[test]
    fn catches_title_over_hundred_chars() {
        let d = draft(&"а".repeat(101), "Описание.");
        assert!(has_error(&check(&d, &cfg()), "длиннее 100 символов"));
    }

    #[test]
    fn flags_superlatives_as_warning_not_error() {
        let d = draft("Стул", "Самая низкая цена в городе.");
        let issues = check(&d, &cfg());
        assert!(issues
            .iter()
            .any(|i| i.severity == Severity::Warning && i.message.contains("превосходная")));
    }

    #[test]
    fn clean_listing_has_no_errors() {
        let description = format!(
            "Продаю кресло из массива дуба в отличном состоянии.\n\n{}",
            "Каркас без трещин, обивка чистая, сколов нет. ".repeat(20)
        );
        let issues = check(&draft("Кресло из массива дуба, отличное состояние", &description), &cfg());
        assert!(
            !issues.iter().any(|i| i.severity == Severity::Error),
            "неожиданные ошибки: {issues:?}"
        );
    }

    #[test]
    fn warns_when_hook_does_not_match_description_start() {
        let mut d = draft("Стул", "Продаю стул из дуба, состояние отличное.");
        d.hook = "Совершенно другой текст в качестве хука для выдачи".into();
        let issues = check(&d, &cfg());
        assert!(issues.iter().any(|i| i.field == "hook"));
    }
}
