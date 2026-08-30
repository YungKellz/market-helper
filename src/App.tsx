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
} from "./api";
import AttributesForm from "./components/AttributesForm";
import FactsPanel from "./components/FactsPanel";
import PhotoPanel from "./components/PhotoPanel";
import ResultPanel from "./components/ResultPanel";
import SettingsDialog from "./components/SettingsDialog";
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
  type UserAttributes,
} from "./types";

type Busy = null | "status" | "analyze" | "generate";

export default function App() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [statuses, setStatuses] = useState<BackendStatus[]>([]);
  const [settingsOpen, setSettingsOpen] = useState(false);

  const [photos, setPhotos] = useState<PhotoInfo[]>([]);
  const [hint, setHint] = useState("");
  const [facts, setFacts] = useState<ProductFacts | null>(null);
  const [attributes, setAttributes] = useState<UserAttributes>(emptyAttributes);
  const [options, setOptions] = useState<GenerateOptions>(defaultOptions);

  const [result, setResult] = useState<ListingResult | null>(null);
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

  useEffect(() => {
    getConfig().then(setConfig).catch((e) => setError(errorText(e)));
    void refreshStatuses();
  }, [refreshStatuses]);

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
    if (next) setResult(next);
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
    if (next) setResult(next);
  }

  // Ручные правки в текстовых полях тоже должны переприменять проверки Авито,
  // но дёргать Rust на каждое нажатие клавиши незачем.
  const latestResult = useRef<ListingResult | null>(null);
  useEffect(() => {
    latestResult.current = result;
  }, [result]);

  const lintTimer = useRef<number | null>(null);
  function patchDraft(patch: { title?: string; description?: string }) {
    setResult((prev) => (prev ? { ...prev, ...patch } : prev));
    if (lintTimer.current !== null) window.clearTimeout(lintTimer.current);

    lintTimer.current = window.setTimeout(async () => {
      const current = latestResult.current;
      if (!current) return;
      const checked = await lintListing({
        title: current.title,
        hook: current.hook,
        description: current.description,
        tags: current.tags,
      });
      // Берём только вычисленные поля: текст мог уйти вперёд, пока ждали ответ.
      setResult((prev) =>
        prev
          ? {
              ...prev,
              title_chars: checked.title_chars,
              description_chars: checked.description_chars,
              issues: checked.issues,
            }
          : prev,
      );
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
        onRefresh={refreshStatuses}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      <div className="columns">
        <div className="column">
          {error && <div className="banner">{error}</div>}

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
            onDraftChange={patchDraft}
            onRefine={refine}
          />
        </div>
      </div>

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
