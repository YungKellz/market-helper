import { useState } from "react";

import { errorText, onPullProgress, pullModel } from "../api";
import type { AppConfig, BackendStatus } from "../types";

interface Props {
  config: AppConfig;
  statuses: BackendStatus[];
  onSave: (config: AppConfig) => void;
  onClose: () => void;
}

/** Пресеты под типичный объём видеопамяти. Q4-квант 8B занимает ~6.5 ГБ. */
const MODEL_PRESETS = [
  { model: "qwen3-vl:8b", note: "8+ ГБ VRAM — лучшее качество распознавания и текста" },
  { model: "qwen3-vl:4b", note: "6 ГБ VRAM — заметно быстрее, текст чуть проще" },
  { model: "qwen2.5vl:3b", note: "4 ГБ VRAM или CPU — минимальные требования" },
];

export default function SettingsDialog({ config, statuses, onSave, onClose }: Props) {
  const [draft, setDraft] = useState<AppConfig>(config);
  const [pulling, setPulling] = useState<string | null>(null);
  const [progress, setProgress] = useState("");
  const [error, setError] = useState<string | null>(null);

  const ollama = statuses.find((s) => s.kind === "ollama");

  const patch = (part: Partial<AppConfig>) => setDraft({ ...draft, ...part });

  async function download(model: string) {
    setPulling(model);
    setProgress("Начинаю загрузку…");
    setError(null);
    const unlisten = await onPullProgress((line) => {
      try {
        const parsed = JSON.parse(line);
        const total = parsed.total as number | undefined;
        const done = parsed.completed as number | undefined;
        setProgress(
          total && done
            ? `${parsed.status}: ${Math.round((done / total) * 100)}%`
            : String(parsed.status ?? line),
        );
      } catch {
        setProgress(line);
      }
    });
    try {
      await pullModel(model);
      setProgress(`Модель ${model} скачана`);
    } catch (e) {
      setError(errorText(e));
      setProgress("");
    } finally {
      unlisten();
      setPulling(null);
    }
  }

  return (
    <div className="backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>Настройки</h2>

        {error && <div className="banner">{error}</div>}

        <label className="field">
          <span>Бэкенд инференса</span>
          <select
            value={draft.backend}
            onChange={(e) => patch({ backend: e.target.value as AppConfig["backend"] })}
          >
            <option value="auto">Автоматически: Ollama, иначе встроенный llama.cpp</option>
            <option value="ollama">Только Ollama</option>
            <option value="llama_cpp">Только встроенный llama-server</option>
          </select>
        </label>

        <div className="grid2">
          <label className="field">
            <span>Модель для распознавания фото</span>
            <input
              value={draft.ollama.vision_model}
              onChange={(e) =>
                patch({ ollama: { ...draft.ollama, vision_model: e.target.value } })
              }
            />
          </label>
          <label className="field">
            <span>Модель для текста</span>
            <input
              value={draft.ollama.text_model}
              onChange={(e) => patch({ ollama: { ...draft.ollama, text_model: e.target.value } })}
            />
          </label>
        </div>
        <p className="hint" style={{ marginTop: -6 }}>
          Одна и та же модель на обоих этапах экономит время: Ollama не перезагружает веса между
          распознаванием и генерацией.
        </p>

        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {MODEL_PRESETS.map((p) => {
            const installed = ollama?.models.some((m) => m === p.model) ?? false;
            return (
              <div className="row" key={p.model}>
                <span className={`dot ${installed ? "ok" : "warn"}`} />
                <span style={{ flex: 1 }}>
                  <b>{p.model}</b>
                  <span className="hint"> — {p.note}</span>
                </span>
                <button
                  className="ghost"
                  onClick={() =>
                    patch({
                      ollama: { ...draft.ollama, vision_model: p.model, text_model: p.model },
                    })
                  }
                >
                  Выбрать
                </button>
                <button
                  className="ghost"
                  onClick={() => download(p.model)}
                  disabled={installed || pulling !== null || !ollama?.available}
                >
                  {installed ? "Скачана" : pulling === p.model ? "Качаю…" : "Скачать"}
                </button>
              </div>
            );
          })}
          {progress && <div className="hint">{progress}</div>}
          {!ollama?.available && (
            <div className="hint">
              Ollama не запущена — скачивание моделей недоступно. Установите её с ollama.com или
              переключитесь на встроенный llama-server.
            </div>
          )}
        </div>

        <label className="field">
          <span>Адрес Ollama</span>
          <input
            value={draft.ollama.base_url}
            onChange={(e) => patch({ ollama: { ...draft.ollama, base_url: e.target.value } })}
          />
        </label>

        <div className="grid2">
          <label className="field">
            <span>llama-server.exe (для встроенного режима)</span>
            <input
              value={draft.llama_cpp.server_binary ?? ""}
              onChange={(e) =>
                patch({
                  llama_cpp: { ...draft.llama_cpp, server_binary: e.target.value || null },
                })
              }
              placeholder="рядом с приложением, подкаталог llm"
            />
          </label>
          <label className="field">
            <span>GGUF-модель</span>
            <input
              value={draft.llama_cpp.model_path ?? ""}
              onChange={(e) =>
                patch({ llama_cpp: { ...draft.llama_cpp, model_path: e.target.value || null } })
              }
              placeholder="C:\\models\\qwen3-vl-8b-q4_k_m.gguf"
            />
          </label>
        </div>
        <label className="field">
          <span>mmproj-файл — без него встроенный режим не увидит фото</span>
          <input
            value={draft.llama_cpp.mmproj_path ?? ""}
            onChange={(e) =>
              patch({ llama_cpp: { ...draft.llama_cpp, mmproj_path: e.target.value || null } })
            }
            placeholder="C:\\models\\mmproj-qwen3-vl-8b-f16.gguf"
          />
        </label>

        <h2 style={{ fontSize: 14 }}>Профиль продавца</h2>
        <div className="grid2">
          <label className="field">
            <span>Кто продаёт</span>
            <select
              value={draft.seller.kind}
              onChange={(e) =>
                patch({
                  seller: { ...draft.seller, kind: e.target.value as "private" | "shop" },
                })
              }
            >
              <option value="private">Частное лицо</option>
              <option value="shop">Магазин или ИП</option>
            </select>
          </label>
          <label className="field">
            <span>Город</span>
            <input
              value={draft.seller.city}
              onChange={(e) => patch({ seller: { ...draft.seller, city: e.target.value } })}
            />
          </label>
          <label className="field">
            <span>Доставка</span>
            <input
              value={draft.seller.delivery}
              onChange={(e) => patch({ seller: { ...draft.seller, delivery: e.target.value } })}
              placeholder="Авито Доставка"
            />
          </label>
          <label className="field">
            <span>Самовывоз</span>
            <input
              value={draft.seller.pickup}
              onChange={(e) => patch({ seller: { ...draft.seller, pickup: e.target.value } })}
              placeholder="метро Технологический институт"
            />
          </label>
        </div>
        <label className="check">
          <input
            type="checkbox"
            checked={draft.seller.bargain}
            onChange={(e) => patch({ seller: { ...draft.seller, bargain: e.target.checked } })}
          />
          Торг уместен
        </label>

        <h2 style={{ fontSize: 14 }}>Генерация</h2>
        <div className="grid2">
          <label className="field">
            <span>Температура: {draft.generation.temperature.toFixed(2)}</span>
            <input
              type="range"
              min={0}
              max={1.2}
              step={0.05}
              value={draft.generation.temperature}
              onChange={(e) =>
                patch({
                  generation: { ...draft.generation, temperature: Number(e.target.value) },
                })
              }
            />
          </label>
          <label className="field">
            <span>Максимальная длина описания, символов</span>
            <input
              type="number"
              value={draft.generation.target_chars_max}
              onChange={(e) =>
                patch({
                  generation: { ...draft.generation, target_chars_max: Number(e.target.value) },
                })
              }
            />
          </label>
        </div>

        <div className="actions">
          <button onClick={onClose}>Отмена</button>
          <button className="primary" onClick={() => onSave(draft)}>
            Сохранить
          </button>
        </div>
      </div>
    </div>
  );
}
