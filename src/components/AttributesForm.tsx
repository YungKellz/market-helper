import type { GenerateOptions, UserAttributes } from "../types";

interface Props {
  attributes: UserAttributes;
  onAttributesChange: (value: UserAttributes) => void;
  options: GenerateOptions;
  onOptionsChange: (value: GenerateOptions) => void;
  disabled: boolean;
}

const TEXT_FIELDS: Array<[keyof UserAttributes, string, string]> = [
  ["title_hint", "Название", "Самокат детский Novatrack"],
  ["price", "Цена, ₽", "4500"],
  ["brand", "Бренд", "Novatrack"],
  ["model", "Модель", "Rainbow 180"],
  ["condition", "Состояние", "хорошее, катались один сезон"],
  ["size", "Размер / габариты", "колёса 180 мм, руль 68–92 см"],
  ["color", "Цвет", "мятный"],
  ["included", "Комплектность", "инструкция, ключ для регулировки"],
];

export default function AttributesForm({
  attributes,
  onAttributesChange,
  options,
  onOptionsChange,
  disabled,
}: Props) {
  const setAttr = (key: keyof UserAttributes, value: string) =>
    onAttributesChange({ ...attributes, [key]: value });

  const setBeauty = <K extends keyof UserAttributes["beauty"]>(
    key: K,
    value: UserAttributes["beauty"][K],
  ) => onAttributesChange({ ...attributes, beauty: { ...attributes.beauty, [key]: value } });

  const setOption = <K extends keyof GenerateOptions>(key: K, value: GenerateOptions[K]) =>
    onOptionsChange({ ...options, [key]: value });

  const setCustom = (index: number, patch: { name?: string; value?: string }) => {
    const custom = attributes.custom.map((c, i) => (i === index ? { ...c, ...patch } : c));
    onAttributesChange({ ...attributes, custom });
  };

  return (
    <section className="section">
      <header>
        <span className="step">3</span>
        <h2>Характеристики — необязательно</h2>
      </header>

      <p className="hint" style={{ marginTop: -4, marginBottom: 12 }}>
        Всё, что заполните здесь, приоритетнее распознанного по фото. Пустые поля просто не попадут
        в текст — модель не станет их выдумывать.
      </p>

      <div className="grid2">
        {TEXT_FIELDS.map(([key, label, placeholder]) => (
          <label className="field" key={key}>
            <span>{label}</span>
            <input
              value={attributes[key] as string}
              onChange={(e) => setAttr(key, e.target.value)}
              placeholder={placeholder}
              disabled={disabled}
            />
          </label>
        ))}
      </div>

      <label className="field" style={{ marginTop: 10 }}>
        <span>Недостатки — их честное перечисление снижает число отказов на встрече</span>
        <input
          value={attributes.defects}
          onChange={(e) => setAttr("defects", e.target.value)}
          placeholder="потёртость на деке, царапина на руле"
          disabled={disabled}
        />
      </label>

      <label className="field" style={{ marginTop: 10 }}>
        <span>Причина продажи</span>
        <input
          value={attributes.reason_for_sale}
          onChange={(e) => setAttr("reason_for_sale", e.target.value)}
          placeholder="ребёнок вырос"
          disabled={disabled}
        />
      </label>

      <details style={{ marginTop: 14 }}>
        <summary style={{ cursor: "pointer", fontWeight: 600 }}>
          Парфюмерия и косметика
        </summary>
        <p className="hint" style={{ marginTop: 6, marginBottom: 10 }}>
          Для мистов, лосьонов и духов. Здесь решает не состояние, а аромат,
          оригинальность и свежесть — заполните, что знаете.
        </p>
        <div className="grid2">
          <label className="field">
            <span>Происхождение</span>
            <input
              value={attributes.beauty.origin}
              onChange={(e) => setBeauty("origin", e.target.value)}
              placeholder="Оригинал, выкуплен в США"
              disabled={disabled}
            />
          </label>
          <label className="field">
            <span>Тип аромата</span>
            <input
              value={attributes.beauty.scent_type}
              onChange={(e) => setBeauty("scent_type", e.target.value)}
              placeholder="фруктово-цветочный"
              disabled={disabled}
            />
          </label>
          <label className="field">
            <span>Срок годности</span>
            <input
              value={attributes.beauty.expiry}
              onChange={(e) => setBeauty("expiry", e.target.value)}
              placeholder="3 года"
              disabled={disabled}
            />
          </label>
          <label className="field">
            <span>Упаковка</span>
            <input
              value={attributes.beauty.sealed}
              onChange={(e) => setBeauty("sealed", e.target.value)}
              placeholder="запечатан / тестер"
              disabled={disabled}
            />
          </label>
        </div>
        <label className="field" style={{ marginTop: 10 }}>
          <span>Ноты аромата</span>
          <input
            value={attributes.beauty.scent_notes}
            onChange={(e) => setBeauty("scent_notes", e.target.value)}
            placeholder="ваниль, яблоневый цвет, мускус"
            disabled={disabled}
          />
        </label>
        <div className="row" style={{ marginTop: 12, flexWrap: "wrap", gap: 14 }}>
          <label className="check">
            <input
              type="checkbox"
              checked={attributes.beauty.batch_code}
              onChange={(e) => setBeauty("batch_code", e.target.checked)}
              disabled={disabled}
            />
            Пришлю фото батч-кода по запросу
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={attributes.beauty.assortment}
              onChange={(e) => setBeauty("assortment", e.target.checked)}
              disabled={disabled}
            />
            Другие ароматы — в профиле
          </label>
        </div>
      </details>

      {attributes.custom.length > 0 && (
        <div style={{ display: "flex", flexDirection: "column", gap: 8, marginTop: 10 }}>
          {attributes.custom.map((c, i) => (
            <div className="row" key={i}>
              <input
                value={c.name}
                onChange={(e) => setCustom(i, { name: e.target.value })}
                placeholder="характеристика"
                disabled={disabled}
              />
              <input
                value={c.value}
                onChange={(e) => setCustom(i, { value: e.target.value })}
                placeholder="значение"
                disabled={disabled}
              />
              <button
                className="ghost"
                onClick={() =>
                  onAttributesChange({
                    ...attributes,
                    custom: attributes.custom.filter((_, j) => j !== i),
                  })
                }
                disabled={disabled}
              >
                ✕
              </button>
            </div>
          ))}
        </div>
      )}

      <button
        className="ghost"
        style={{ marginTop: 10 }}
        onClick={() =>
          onAttributesChange({ ...attributes, custom: [...attributes.custom, { name: "", value: "" }] })
        }
        disabled={disabled}
      >
        + Своя характеристика
      </button>

      <label className="field" style={{ marginTop: 12 }}>
        <span>Что ещё важно сказать покупателю</span>
        <textarea
          rows={3}
          value={attributes.notes}
          onChange={(e) => setAttr("notes", e.target.value)}
          placeholder="Покупали в официальном магазине, сохранился чек"
          disabled={disabled}
        />
      </label>

      <div className="grid2" style={{ marginTop: 14 }}>
        <label className="field">
          <span>Тон описания</span>
          <select
            value={options.tone}
            onChange={(e) => setOption("tone", e.target.value as GenerateOptions["tone"])}
            disabled={disabled}
          >
            <option value="friendly">Живой, от частного продавца</option>
            <option value="business">Деловой, от магазина</option>
            <option value="concise">Сухой и короткий</option>
          </select>
        </label>
        <label className="field">
          <span>Целевая аудитория</span>
          <input
            value={options.audience}
            onChange={(e) => setOption("audience", e.target.value)}
            placeholder="родители школьников"
            disabled={disabled}
          />
        </label>
      </div>

      <div className="row" style={{ marginTop: 12, flexWrap: "wrap", gap: 14 }}>
        <label className="check">
          <input
            type="checkbox"
            checked={options.include_cta}
            onChange={(e) => setOption("include_cta", e.target.checked)}
            disabled={disabled}
          />
          Призыв к действию
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={options.include_tags}
            onChange={(e) => setOption("include_tags", e.target.checked)}
            disabled={disabled}
          />
          Поисковые фразы
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={options.disclose_defects}
            onChange={(e) => setOption("disclose_defects", e.target.checked)}
            disabled={disabled}
          />
          Отдельный блок про состояние
        </label>
      </div>
    </section>
  );
}
