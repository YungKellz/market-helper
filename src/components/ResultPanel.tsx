import { useState } from "react";

import type { Issue, ListingResult } from "../types";

interface Props {
  result: ListingResult | null;
  stream: string;
  busy: boolean;
  onDraftChange: (patch: { title?: string; description?: string }) => void;
  onRefine: (instruction: string) => void;
}

const SEVERITY_LABEL: Record<Issue["severity"], string> = {
  error: "Снимут с публикации",
  warning: "Стоит поправить",
};

/** Быстрые правки, которые чаще всего просят после первой генерации. */
const QUICK_FIXES = [
  "Сделай короче на треть, убери воду",
  "Усиль первые две строки — они видны в поиске",
  "Добавь больше конкретики в характеристики",
  "Смени тон на более деловой",
];

function Counter({ value, limit }: { value: number; limit: number }) {
  return (
    <span className={`counter${value > limit ? " over" : ""}`}>
      {value} / {limit}
    </span>
  );
}

export default function ResultPanel({ result, stream, busy, onDraftChange, onRefine }: Props) {
  const [instruction, setInstruction] = useState("");
  const [copied, setCopied] = useState<string | null>(null);

  async function copy(what: string, text: string) {
    await navigator.clipboard.writeText(text);
    setCopied(what);
    setTimeout(() => setCopied(null), 1500);
  }

  if (!result) {
    return (
      <section className="section" style={{ flex: 1, display: "flex", flexDirection: "column" }}>
        <header>
          <span className="step">4</span>
          <h2>Готовая карточка</h2>
        </header>
        {busy && stream ? (
          <div className="stream">{stream}</div>
        ) : (
          <div className="empty">
            {busy
              ? "Модель думает. Первый запрос дольше остальных — веса грузятся в видеопамять."
              : "Загрузите фото, при желании заполните характеристики и нажмите «Сгенерировать описание»."}
          </div>
        )}
      </section>
    );
  }

  const errors = result.issues.filter((i) => i.severity === "error");
  const warnings = result.issues.filter((i) => i.severity === "warning");

  return (
    <>
      <section className="section">
        <header>
          <span className="step">4</span>
          <h2>Заголовок</h2>
          <div className="spacer" />
          <Counter value={result.title_chars} limit={100} />
          <button className="ghost" onClick={() => copy("title", result.title)}>
            {copied === "title" ? "Скопировано" : "Копировать"}
          </button>
        </header>
        <input
          className="result-title"
          value={result.title}
          onChange={(e) => onDraftChange({ title: e.target.value })}
        />
      </section>

      <section className="section">
        <header>
          <h2>Описание</h2>
          <div className="spacer" />
          <Counter value={result.description_chars} limit={1500} />
          <button className="ghost" onClick={() => copy("description", result.description)}>
            {copied === "description" ? "Скопировано" : "Копировать"}
          </button>
        </header>
        <textarea
          rows={16}
          value={result.description}
          onChange={(e) => onDraftChange({ description: e.target.value })}
        />
        <p className="hint" style={{ marginTop: 8, marginBottom: 0 }}>
          В поисковой выдаче видны только первые ~200 символов — они уже выделены в первый абзац.
        </p>
      </section>

      {result.tags.length > 0 && (
        <section className="section">
          <header>
            <h2>Поисковые фразы</h2>
            <div className="spacer" />
            <button className="ghost" onClick={() => copy("tags", result.tags.join(", "))}>
              {copied === "tags" ? "Скопировано" : "Копировать"}
            </button>
          </header>
          <div className="tags">
            {result.tags.map((t, i) => (
              <span className="tag" key={i}>
                {t}
              </span>
            ))}
          </div>
          <p className="hint" style={{ marginTop: 10, marginBottom: 0 }}>
            Не вставляйте их списком в конец описания — за спам-перечисление Авито понижает
            объявление. Используйте как подсказку, куда вплести формулировки.
          </p>
        </section>
      )}

      <section className="section">
        <header>
          <h2>Проверка по правилам Авито</h2>
          <div className="spacer" />
          <span className="counter">
            {errors.length} ошибок · {warnings.length} замечаний
          </span>
        </header>
        {result.issues.length === 0 ? (
          <p className="hint" style={{ margin: 0 }}>
            Нарушений не нашлось: контактов и ссылок нет, длина в норме, заголовок укладывается
            в лимит.
          </p>
        ) : (
          <div className="issues">
            {[...errors, ...warnings].map((issue, i) => (
              <div className={`issue ${issue.severity}`} key={i}>
                <span className={`dot ${issue.severity === "error" ? "err" : "warn"}`} />
                <span>
                  <b>{SEVERITY_LABEL[issue.severity]}.</b> {issue.message}
                  {issue.excerpt && (
                    <>
                      {" "}
                      <code>«{issue.excerpt}»</code>
                    </>
                  )}
                </span>
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="section">
        <header>
          <h2>Переделать</h2>
        </header>
        <div className="row">
          <input
            value={instruction}
            onChange={(e) => setInstruction(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && instruction.trim() && !busy) onRefine(instruction);
            }}
            placeholder="Что поправить в тексте?"
            disabled={busy}
          />
          <button
            className="primary"
            onClick={() => onRefine(instruction)}
            disabled={busy || !instruction.trim()}
          >
            Применить
          </button>
        </div>
        <div className="row" style={{ marginTop: 10, flexWrap: "wrap" }}>
          {QUICK_FIXES.map((q) => (
            <button className="ghost" key={q} onClick={() => setInstruction(q)} disabled={busy}>
              {q}
            </button>
          ))}
        </div>
        {busy && stream && (
          <div className="stream" style={{ marginTop: 12 }}>
            {stream}
          </div>
        )}
      </section>
    </>
  );
}
