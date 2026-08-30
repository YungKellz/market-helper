import type { ProductFacts } from "../types";

interface Props {
  facts: ProductFacts | null;
  hint: string;
  onHintChange: (value: string) => void;
  onAnalyze: () => void;
  busy: boolean;
  canAnalyze: boolean;
}

const FIELDS: Array<[keyof ProductFacts, string]> = [
  ["product_type", "Тип"],
  ["category", "Категория"],
  ["brand", "Бренд"],
  ["model", "Модель"],
  ["color", "Цвет"],
  ["material", "Материал"],
  ["condition", "Состояние"],
  ["size", "Размер"],
];

const LISTS: Array<[keyof ProductFacts, string]> = [
  ["features", "Особенности"],
  ["included", "Комплект"],
  ["defects", "Дефекты"],
];

export default function FactsPanel({
  facts,
  hint,
  onHintChange,
  onAnalyze,
  busy,
  canAnalyze,
}: Props) {
  // Ниже 0.5 модель сама сигналит, что угадывала, — не даём этому уехать в текст молча.
  const lowConfidence = facts !== null && facts.confidence < 0.5;

  return (
    <section className="section">
      <header>
        <span className="step">2</span>
        <h2>Что за товар</h2>
        <div className="spacer" />
        <button className="primary" onClick={onAnalyze} disabled={busy || !canAnalyze}>
          {busy ? "Распознаю…" : facts ? "Распознать заново" : "Распознать по фото"}
        </button>
      </header>

      <label className="field">
        <span>Подсказка модели (необязательно)</span>
        <input
          value={hint}
          onChange={(e) => onHintChange(e.target.value)}
          placeholder="например: детский самокат, алюминиевый"
        />
      </label>

      {!facts && (
        <p className="hint" style={{ marginTop: 12, marginBottom: 0 }}>
          Модель определит тип товара, бренд, состояние и прочитает надписи на этикетках. Всё
          распознанное можно поправить руками ниже.
        </p>
      )}

      {facts && (
        <>
          <div className="facts" style={{ marginTop: 12 }}>
            {FIELDS.map(([key, label]) => {
              const value = facts[key];
              if (typeof value !== "string" || value.trim() === "") return null;
              return (
                <span className="fact" key={key}>
                  <b>{label}:</b> {value}
                </span>
              );
            })}
            {LISTS.map(([key, label]) => {
              const value = facts[key];
              if (!Array.isArray(value) || value.length === 0) return null;
              return (
                <span className="fact" key={key}>
                  <b>{label}:</b> {value.join(", ")}
                </span>
              );
            })}
            <span className="fact">
              <b>Уверенность:</b> {Math.round(facts.confidence * 100)}%
            </span>
          </div>

          {facts.visible_text.length > 0 && (
            <p className="hint" style={{ marginTop: 10, marginBottom: 0 }}>
              Прочитано на товаре: {facts.visible_text.join(" · ")}
            </p>
          )}

          {(lowConfidence || facts.questions.length > 0) && (
            <ul className="questions">
              {lowConfidence && (
                <li>
                  Модель не уверена в распознавании — проверьте характеристики перед генерацией.
                </li>
              )}
              {facts.questions.map((q, i) => (
                <li key={i}>{q}</li>
              ))}
            </ul>
          )}
        </>
      )}
    </section>
  );
}
