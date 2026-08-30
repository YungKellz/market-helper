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
    /// Готовая инструкция модели для кнопки «Исправить». Живёт рядом с самим
    /// правилом: кто знает о нарушении, тот знает и как его чинить.
    pub fix: String,
}

impl Issue {
    fn error(field: &str, message: impl Into<String>, excerpt: Option<String>, fix: &str) -> Self {
        Self {
            severity: Severity::Error,
            field: field.into(),
            message: message.into(),
            excerpt,
            fix: fix.into(),
        }
    }

    fn warning(field: &str, message: impl Into<String>, excerpt: Option<String>, fix: &str) -> Self {
        Self {
            severity: Severity::Warning,
            field: field.into(),
            message: message.into(),
            excerpt,
            fix: fix.into(),
        }
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
static SHOUTING: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b[А-ЯЁA-Z]{4,}\b").unwrap());

const FIX_CONTACTS: &str = "Удали из текста любые контактные данные: номера телефонов, e-mail, ссылки, адреса сайтов и ники в мессенджерах. Авито запрещает их в объявлении.";
const FIX_PLATFORMS: &str = "Убери упоминания других торговых площадок и маркетплейсов.";
const FIX_SUPERLATIVES: &str = "Замени необоснованные превосходные степени вроде «самый дешёвый» на проверяемые факты о товаре.";
const FIX_SHOUTING: &str = "Перепиши слова, написанные целиком заглавными буквами, обычным регистром.";
const FIX_BANGS: &str = "Оставь не больше одного восклицательного знака подряд.";

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
                issues.push(Issue::error(field, message, Some(found), FIX_CONTACTS));
            }
        }
        if let Some(found) = excerpt_of(&OTHER_PLATFORMS, text) {
            issues.push(Issue::error(
                field,
                "упоминание другой торговой площадки — модерация Авито это снимает",
                Some(found),
                FIX_PLATFORMS,
            ));
        }
        if let Some(found) = excerpt_of(&SUPERLATIVES, text) {
            issues.push(Issue::warning(
                field,
                "необоснованная превосходная степень: снижает доверие и попадает под правила о недостоверной рекламе",
                Some(found),
                FIX_SUPERLATIVES,
            ));
        }
        if let Some(found) = excerpt_of(&SHOUTING, text) {
            issues.push(Issue::warning(
                field,
                "слово целиком капсом — Авито считает это привлечением внимания",
                Some(found),
                FIX_SHOUTING,
            ));
        }
        if let Some(found) = excerpt_of(&BANG_RUN, text) {
            issues.push(Issue::warning(
                field,
                "несколько восклицательных знаков подряд",
                Some(found),
                FIX_BANGS,
            ));
        }
    }

    let title_chars = title.chars().count();
    if title_chars == 0 {
        issues.push(Issue::error(
            "title",
            "заголовок пустой",
            None,
            "Придумай заголовок: тип товара, бренд, модель и один-два ключевых параметра.",
        ));
    } else if title_chars > 100 {
        issues.push(Issue::error(
            "title",
            format!("заголовок длиннее 100 символов ({title_chars}) — Авито его обрежет"),
            None,
            "Сократи заголовок до 100 символов: оставь только тип товара, бренд, модель и один-два ключевых параметра.",
        ));
    }
    if let Some(found) = excerpt_of(&PRICE_IN_TITLE, title) {
        issues.push(Issue::error(
            "title",
            "цена в заголовке запрещена — для неё есть отдельное поле",
            Some(found),
            "Убери цену из заголовка — на Авито для неё отдельное поле.",
        ));
    }

    let desc_chars = desc.chars().count();
    if desc_chars == 0 {
        issues.push(Issue::error(
            "description",
            "описание пустое",
            None,
            "Напиши описание по структуре из системного сообщения.",
        ));
    } else if desc_chars < cfg.target_chars_min as usize {
        issues.push(Issue::warning(
            "description",
            format!(
                "описание короче рекомендованных {} символов ({desc_chars}) — не хватает деталей для доверия",
                cfg.target_chars_min
            ),
            None,
            &format!(
                "Расширь описание до {}–{} символов: добавь конкретики о состоянии, комплектности и выгоде для покупателя. Не выдумывай фактов, которых нет в тексте.",
                cfg.target_chars_min, cfg.target_chars_max
            ),
        ));
    } else if desc_chars > cfg.target_chars_max as usize {
        issues.push(Issue::warning(
            "description",
            format!(
                "описание длиннее рекомендованных {} символов ({desc_chars}) — такой текст редко дочитывают",
                cfg.target_chars_max
            ),
            None,
            &format!(
                "Сократи описание до {} символов, убрав повторы и общие фразы. Факты и цифры сохрани.",
                cfg.target_chars_max
            ),
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
                "Сократи первое предложение описания до 200 символов, чтобы оно целиком помещалось в поисковую выдачу, и повтори его дословно в поле hook.",
            ));
        }
        let hook_prefix: String = hook.chars().take(40).collect();
        if !desc.starts_with(&hook_prefix) {
            issues.push(Issue::warning(
                "hook",
                "хук не совпадает с началом описания — в выдаче покупатель увидит другой текст",
                None,
                "Сделай так, чтобы описание начиналось ровно с текста хука: первые 200 символов описания должны дословно совпадать с полем hook.",
            ));
        }
    }

    if draft.tags.len() > 12 {
        issues.push(Issue::warning(
            "tags",
            format!("{} ключевых фраз — переизбыток тегов ведёт к пессимизации", draft.tags.len()),
            None,
            "Оставь не больше десяти поисковых фраз — самых частотных.",
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

    /// Кнопка «Исправить» в интерфейсе отправляет модели `fix` как есть,
    /// поэтому пустая инструкция означала бы мёртвую кнопку.
    #[test]
    fn every_issue_carries_a_fix_instruction() {
        let mut d = draft(
            "СРОЧНО продам диван 25 000 руб",
            "Звоните 89991234567, пишите в телеграм. Самая низкая цена!! Смотрите на example.ru и на озоне.",
        );
        d.hook = "Хук, не совпадающий с началом описания, специально длинный".into();
        d.tags = (0..15).map(|i| format!("фраза {i}")).collect();

        let issues = check(&d, &cfg());
        assert!(issues.len() > 8, "правила не сработали: {issues:?}");
        for issue in &issues {
            assert!(
                !issue.fix.trim().is_empty(),
                "у правила «{}» нет инструкции для исправления",
                issue.message
            );
        }
    }
}
