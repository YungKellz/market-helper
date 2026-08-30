import { useCallback, useEffect, useRef, useState } from "react";

import {
  analyzePhotos,
  backendStatus,
  errorText,
  generateListing,
  getConfig,
  lintListing,
  onToken,
  refineListing,
  saveConfig,
  setupStatus,
} from "./api";
import AttributesForm from "./components/AttributesForm";
import FactsPanel from "./components/FactsPanel";
import PhotoPanel from "./components/PhotoPanel";
import ResultPanel from "./components/ResultPanel";
import SettingsDialog from "./components/SettingsDialog";
import SetupWizard from "./components/SetupWizard";
import StatusBar from "./components/StatusBar";
import {
  defaultOptions,
  emptyAttributes,
  type AppConfig,
  type BackendStatus,
  type GenerateOptions,
  type ListingResult,
  type PhotoInfo,
  type ProductFacts,
  type SetupStatus,
  type UserAttributes,
} from "./types";

type Busy = null | "status" | "analyze" | "generate";

export default function App() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [statuses, setStatuses] = useState<BackendStatus[]>([]);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [setup, setSetup] = useState<SetupStatus | null>(null);
  const [wizardOpen, setWizardOpen] = useState(false);

  const [photos, setPhotos] = useState<PhotoInfo[]>([]);
  const [hint, setHint] = useState("");
  const [facts, setFacts] = useState<ProductFacts | null>(null);
  const [attributes, setAttributes] = useState<UserAttributes>(emptyAttributes);
  const [options, setOptions] = useState<GenerateOptions>(defaultOptions);

  // История вариантов: неудачная перегенерация не должна стирать удачную.
  const [history, setHistory] = useState<ListingResult[]>([]);
  const [cursor, setCursor] = useState(-1);
  const result = cursor >= 0 ? (history[cursor] ?? null) : null;

  const [stream, setStream] = useState("");
  const [busy, setBusy] = useState<Busy>(null);
  const [error, setError] = useState<string | null>(null);

  const refreshStatuses = useCallback(async () => {
    try {
      setStatuses(await backendStatus());
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  const refreshSetup = useCallback(async () => {
    const next = await setupStatus();
    setSetup(next);
    return next;
  }, []);

  useEffect(() => {
    getConfig().then(setConfig).catch((e) => setError(errorText(e)));
    void refreshStatuses();
    // Мастер открывается сам, если движка или модели ещё нет: без них
    // приложение всё равно ничего не сгенерирует.
    refreshSetup()
      .then((s) => setWizardOpen(s.needs_setup))
      .catch((e) => setError(errorText(e)));
  }, [refreshStatuses, refreshSetup]);

  function pushResult(next: ListingResult) {
    // Новый вариант обрезает «будущее»: если пользователь отлистал назад
    // и сгенерировал заново, ветка вперёд теряет смысл.
    setHistory((prev) => [...prev.slice(0, cursor + 1), next]);
    setCursor(cursor + 1);
  }

  function updateAt(index: number, patch: Partial<ListingResult>) {
    setHistory((prev) => prev.map((r, i) => (i === index ? { ...r, ...patch } : r)));
  }

  async function withStream<T>(kind: Busy, run: () => Promise<T>): Promise<T | null> {
    setBusy(kind);
    setStream("");
    setError(null);
    const unlisten = await onToken((chunk) => setStream((s) => s + chunk));
    try {
      return await run();
    } catch (e) {
      setError(errorText(e));
      return null;
    } finally {
      unlisten();
      setBusy(null);
      setStream("");
    }
  }

  async function analyze() {
    setBusy("analyze");
    setError(null);
    try {
      setFacts(await analyzePhotos(photos.map((p) => p.id), hint));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  }

  async function generate() {
    // Распознавание необязательно: карточку можно собрать и по одним
    // характеристикам, введённым руками.
    const source = facts ?? {
      ...emptyFactsFromAttributes(attributes),
    };
    const next = await withStream("generate", () => generateListing(source, attributes, options));
    if (next) pushResult(next);
  }

  async function refine(instruction: string) {
    if (!result) return;
    const next = await withStream("generate", () =>
      refineListing(
        {
          title: result.title,
          hook: result.hook,
          description: result.description,
          tags: result.tags,
        },
        instruction,
        options,
      ),
    );
    if (next) pushResult(next);
  }

  // Ручные правки в текстовых полях тоже должны переприменять проверки Авито,
  // но дёргать Rust на каждое нажатие клавиши незачем.
  const lintTimer = useRef<number | null>(null);
  function patchDraft(patch: { title?: string; description?: string }) {
    const index = cursor;
    const base = history[index];
    if (!base) return;

    const updated = { ...base, ...patch };
    // Хук — производная от описания, держим её в согласованном виде и здесь.
    updated.hook = updated.description.slice(0, 200);
    updateAt(index, { ...patch, hook: updated.hook });

    // Каждое нажатие клавиши отменяет прошлый таймер, поэтому до Rust
    // доедет только последняя редакция — гонки за отставший ответ нет.
    if (lintTimer.current !== null) window.clearTimeout(lintTimer.current);
    lintTimer.current = window.setTimeout(async () => {
      const checked = await lintListing({
        title: updated.title,
        hook: updated.hook,
        description: updated.description,
        tags: updated.tags,
      });
      updateAt(index, {
        title_chars: checked.title_chars,
        description_chars: checked.description_chars,
        issues: checked.issues,
      });
    }, 400);
  }

  async function persistConfig(next: AppConfig) {
    try {
      await saveConfig(next);
      setConfig(next);
      setSettingsOpen(false);
      await refreshStatuses();
    } catch (e) {
      setError(errorText(e));
    }
  }

  const canGenerate =
    busy === null && (facts !== null || attributes.title_hint.trim().length > 0);

  return (
    <div className="app">
      <StatusBar
        statuses={statuses}
        busy={busy !== null}
        needsSetup={setup?.needs_setup ?? false}
        onOpenSetup={() => setWizardOpen(true)}
        onRefresh={() => {
          void refreshStatuses();
          void refreshSetup();
        }}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      {/* Полосой во всю ширину, а не внутри колонки: раньше баннер жил над
          блоком с фото и при прокрутке уезжал из виду — сбой генерации
          выглядел как «ничего не произошло». */}
      {error && (
        <div className="banner banner-top">
          <span>{error}</span>
          <button className="ghost" onClick={() => setError(null)} title="Скрыть">
            ✕
          </button>
        </div>
      )}

      <div className="columns">
        <div className="column">
          <PhotoPanel
            photos={photos}
            onChange={setPhotos}
            onError={setError}
            disabled={busy !== null}
          />

          <FactsPanel
            facts={facts}
            hint={hint}
            onHintChange={setHint}
            onAnalyze={analyze}
            busy={busy === "analyze"}
            canAnalyze={photos.length > 0 && busy === null}
          />

          <AttributesForm
            attributes={attributes}
            onAttributesChange={setAttributes}
            options={options}
            onOptionsChange={setOptions}
            disabled={busy !== null}
          />

          <button className="primary" onClick={generate} disabled={!canGenerate}>
            {busy === "generate" ? "Генерирую описание…" : "Сгенерировать описание"}
          </button>
          {!canGenerate && busy === null && (
            <p className="hint" style={{ textAlign: "center" }}>
              Сначала распознайте товар по фото или заполните поле «Название».
            </p>
          )}
        </div>

        <div className="column">
          <ResultPanel
            result={result}
            stream={stream}
            busy={busy === "generate"}
            historyIndex={cursor}
            historyTotal={history.length}
            onNavigate={(delta) =>
              setCursor((c) => Math.min(history.length - 1, Math.max(0, c + delta)))
            }
            onDraftChange={patchDraft}
            onRefine={refine}
          />
        </div>
      </div>

      {wizardOpen && setup && (
        <SetupWizard
          status={setup}
          onReady={() => {
            setWizardOpen(false);
            void refreshStatuses();
            void refreshSetup();
          }}
          onSkip={() => setWizardOpen(false)}
        />
      )}

      {settingsOpen && config && (
        <SettingsDialog
          config={config}
          statuses={statuses}
          onSave={persistConfig}
          onClose={() => setSettingsOpen(false)}
        />
      )}
    </div>
  );
}

/** Генерация без распознавания: подставляем то, что человек ввёл руками. */
function emptyFactsFromAttributes(attributes: UserAttributes): ProductFacts {
  return {
    category: "",
    product_type: attributes.title_hint,
    brand: attributes.brand,
    model: attributes.model,
    color: attributes.color,
    material: "",
    condition: attributes.condition,
    defects: attributes.defects ? [attributes.defects] : [],
    features: [],
    included: attributes.included ? [attributes.included] : [],
    size: attributes.size,
    visible_text: [],
    confidence: 1,
    uncertain: [],
    questions: [],
  };
}
