use serde::{Deserialize, Serialize};

/// Что модель увидела на фото. Всё опционально: пустое поле честнее выдумки.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProductFacts {
    /// Предполагаемая категория Авито, например «Телефоны» или «Одежда, обувь».
    pub category: String,
    /// Тип товара обиходным словом: «кроссовки», «дрель», «диван».
    pub product_type: String,
    pub brand: String,
    pub model: String,
    pub color: String,
    pub material: String,
    /// новое | отличное | хорошее | удовлетворительное
    pub condition: String,
    /// Видимые дефекты: потёртости, сколы, отсутствующие детали.
    pub defects: Vec<String>,
    /// Заметные особенности, которые видно на фото.
    pub features: Vec<String>,
    /// Комплектность: коробка, зарядка, документы.
    pub included: Vec<String>,
    /// Габариты/размер, если их можно прочитать или оценить.
    pub size: String,
    /// Надписи, прочитанные на этикетках и корпусе, — сырьё для проверки модели.
    pub visible_text: Vec<String>,
    /// 0.0–1.0. Ниже 0.5 — интерфейс просит пользователя уточнить руками.
    pub confidence: f32,
    /// Поля, в которых модель не уверена.
    pub uncertain: Vec<String>,
    /// Вопросы к пользователю, которые заметно улучшат описание.
    pub questions: Vec<String>,
}

/// Необязательные характеристики, которые пользователь заполняет руками.
/// Они всегда приоритетнее того, что «увидела» модель.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UserAttributes {
    pub title_hint: String,
    pub brand: String,
    pub model: String,
    pub condition: String,
    pub price: String,
    pub size: String,
    pub color: String,
    pub included: String,
    pub defects: String,
    pub reason_for_sale: String,
    /// Поля, значимые только для парфюмерии и косметики. Для остальных товаров
    /// остаются пустыми и в текст не попадают.
    pub beauty: BeautyAttributes,
    /// Произвольные пары «характеристика — значение».
    pub custom: Vec<CustomAttribute>,
    /// Свободный текст: всё, что пользователь хочет донести.
    pub notes: String,
}

/// Специфика ниши парфюмерии/косметики (в первую очередь Victoria's Secret):
/// то, что решает продажу здесь, но бессмысленно для техники или мебели.
/// Всё опционально — пустое поле в описание не попадает.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BeautyAttributes {
    /// Происхождение: «Оригинал, выкуплен в США», «Привезено из Европы».
    pub origin: String,
    /// Тип аромата: фруктово-цветочный, древесно-гурманский, сладкий ванильный.
    pub scent_type: String,
    /// Ноты аромата. Пирамиду можно записать текстом.
    pub scent_notes: String,
    /// Срок годности: «3 года», «до 2027».
    pub expiry: String,
    /// Состояние упаковки: «запечатан», «вскрыт», «тестер», «миниатюра».
    pub sealed: String,
    /// Продавец готов прислать фото батч-кода для проверки оригинальности.
    pub batch_code: bool,
    /// У продавца есть другие ароматы — приглашаем посмотреть профиль.
    pub assortment: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomAttribute {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GenerateOptions {
    /// friendly | business | concise
    pub tone: String,
    /// Целевая аудитория, например «родители школьников».
    pub audience: String,
    pub include_cta: bool,
    pub include_tags: bool,
    /// Раскрывать ли дефекты явным блоком. По ресерчу — честность повышает
    /// конверсию в сделку и снижает возвраты, поэтому по умолчанию включено.
    pub disclose_defects: bool,
}

impl Default for GenerateOptions {
    fn default() -> Self {
        Self {
            tone: "friendly".into(),
            audience: String::new(),
            include_cta: true,
            include_tags: true,
            disclose_defects: true,
        }
    }
}

/// Готовая карточка.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ListingDraft {
    /// До 100 символов, без цены и контактов.
    pub title: String,
    /// Первые ~200 символов описания — только они видны в выдаче.
    pub hook: String,
    /// Полный текст, готовый к вставке в поле «Описание».
    pub description: String,
    pub tags: Vec<String>,
}

/// Результат генерации вместе с проверками по правилам Авито.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ListingResult {
    #[serde(flatten)]
    pub draft: ListingDraft,
    pub title_chars: usize,
    pub description_chars: usize,
    pub issues: Vec<crate::pipeline::lint::Issue>,
    /// Какой бэкенд обслужил запрос — в режиме `auto` это неочевидно.
    pub backend: String,
}
