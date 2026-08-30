import type { BackendStatus } from "../types";

const LABEL: Record<string, string> = {
  ollama: "Ollama",
  llama_cpp: "Встроенный llama.cpp",
};

/** Зелёный — можно генерировать, жёлтый — сервис есть, но модели нет. */
function dotClass(s: BackendStatus): string {
  if (!s.available) return "err";
  return s.vision_model_ready && s.text_model_ready ? "ok" : "warn";
}

interface Props {
  statuses: BackendStatus[];
  busy: boolean;
  /** Движок или модель ещё не установлены — предлагаем вернуться в мастер. */
  needsSetup: boolean;
  onOpenSetup: () => void;
  onRefresh: () => void;
  onOpenSettings: () => void;
}

export default function StatusBar({
  statuses,
  busy,
  needsSetup,
  onOpenSetup,
  onRefresh,
  onOpenSettings,
}: Props) {
  return (
    <div className="topbar">
      <h1>Market Helper</h1>
      <span className="hint">описания карточек для Авито</span>
      <div className="spacer" />

      {statuses.map((s) => (
        <span className="chip" key={s.kind} title={`${s.endpoint} — ${s.detail}`}>
          <span className={`dot ${dotClass(s)}`} />
          {LABEL[s.kind] ?? s.kind}
          {s.version ? ` ${s.version}` : ""}
        </span>
      ))}

      {needsSetup && (
        <button className="primary" onClick={onOpenSetup} disabled={busy}>
          Завершить установку
        </button>
      )}

      <button className="ghost" onClick={onRefresh} disabled={busy} title="Проверить бэкенды заново">
        Обновить
      </button>
      <button className="ghost" onClick={onOpenSettings}>
        Настройки
      </button>
    </div>
  );
}
