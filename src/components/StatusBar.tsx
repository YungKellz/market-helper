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
  onRefresh: () => void;
  onOpenSettings: () => void;
}

export default function StatusBar({ statuses, busy, onRefresh, onOpenSettings }: Props) {
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

      <button className="ghost" onClick={onRefresh} disabled={busy} title="Проверить бэкенды заново">
        Обновить
      </button>
      <button className="ghost" onClick={onOpenSettings}>
        Настройки
      </button>
    </div>
  );
}
