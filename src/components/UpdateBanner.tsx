import { useState } from "react";

import { errorText, installUpdate } from "../api";
import type { PendingUpdate } from "../types";

interface Props {
  update: PendingUpdate;
  onDismiss: () => void;
}

/** Полоса под шапкой: приложение нашло свежую версию и предлагает её поставить. */
export default function UpdateBanner({ update, onDismiss }: Props) {
  const [percent, setPercent] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function install() {
    setBusy(true);
    setError(null);
    try {
      // После установки приложение перезапускается само, поэтому кода
      // после этого вызова уже не будет.
      await installUpdate(update, setPercent);
    } catch (e) {
      setError(errorText(e));
      setBusy(false);
      setPercent(null);
    }
  }

  return (
    <div className="update-bar">
      <span className="dot ok" />
      <span style={{ flex: 1 }}>
        {error ? (
          <>Не удалось обновиться: {error}</>
        ) : busy ? (
          <>
            Ставлю версию {update.version}
            {percent !== null ? ` — ${percent}%` : "…"}
          </>
        ) : (
          <>
            Вышла версия {update.version}. Обновление поставится поверх текущей,
            приложение перезапустится.
          </>
        )}
      </span>

      {busy && percent !== null && (
        <div className="progress" style={{ width: 160 }}>
          <div className="progress-fill" style={{ width: `${percent}%` }} />
        </div>
      )}

      <button className="primary" onClick={install} disabled={busy}>
        {error ? "Повторить" : busy ? "Ставлю…" : "Обновить"}
      </button>
      <button className="ghost" onClick={onDismiss} disabled={busy} title="Скрыть до следующего запуска">
        ✕
      </button>
    </div>
  );
}
