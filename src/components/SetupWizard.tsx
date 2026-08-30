import { useEffect, useRef, useState } from "react";

import {
  errorText,
  installOllama,
  onPullProgress,
  onSetupProgress,
  pullModel,
  setupStatus,
  startOllama,
} from "../api";
import type { SetupStatus } from "../types";

type StepId = "install" | "start" | "model";
type StepState = "pending" | "running" | "done";

interface Props {
  status: SetupStatus;
  onReady: () => void;
  onSkip: () => void;
}

const STEPS: Array<{ id: StepId; title: string; note: string }> = [
  {
    id: "install",
    title: "Установить движок Ollama",
    note: "Программа, которая запускает нейросеть на вашем компьютере. Около 1 ГБ, ставится один раз.",
  },
  {
    id: "start",
    title: "Запустить движок",
    note: "Ollama работает в фоне и запускается вместе с Windows.",
  },
  {
    id: "model",
    title: "Скачать модель",
    note: "Сама нейросеть, около 6 ГБ. Скачивается один раз, дальше всё работает без интернета.",
  },
];

function stateOf(step: StepId, status: SetupStatus): StepState {
  if (step === "install") return status.ollama_installed ? "done" : "pending";
  if (step === "start") return status.ollama_running ? "done" : "pending";
  return status.model_ready ? "done" : "pending";
}

export default function SetupWizard({ status: initial, onReady, onSkip }: Props) {
  const [status, setStatus] = useState(initial);
  const [active, setActive] = useState<StepId | null>(null);
  const [message, setMessage] = useState("");
  const [percent, setPercent] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Слушатели живут всё время, пока открыт мастер: события прилетают из трёх
  // разных мест, и переподписываться на каждом шаге незачем.
  const cleanup = useRef<Array<() => void>>([]);
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const offSetup = await onSetupProgress((p) => {
        setMessage(p.message);
        setPercent(p.percent);
      });
      const offPull = await onPullProgress((line) => {
        try {
          const parsed = JSON.parse(line);
          const total: number | undefined = parsed.total;
          const done: number | undefined = parsed.completed;
          setMessage(String(parsed.status ?? "Скачиваю модель…"));
          setPercent(total && done ? Math.round((done / total) * 100) : null);
        } catch {
          setMessage(line);
        }
      });
      if (cancelled) {
        offSetup();
        offPull();
        return;
      }
      cleanup.current = [offSetup, offPull];
    })();

    return () => {
      cancelled = true;
      cleanup.current.forEach((off) => off());
      cleanup.current = [];
    };
  }, []);

  /** Прогоняет все незавершённые шаги подряд — пользователь жмёт одну кнопку. */
  async function run() {
    setError(null);
    let current = status;

    try {
      if (!current.ollama_installed) {
        setActive("install");
        await installOllama();
      }

      setActive("start");
      await startOllama();

      current = await setupStatus();
      setStatus(current);

      if (!current.model_ready) {
        setActive("model");
        setMessage("Скачиваю модель…");
        await pullModel(current.model);
      }

      const final = await setupStatus();
      setStatus(final);
      setActive(null);
      setPercent(null);

      if (!final.needs_setup) {
        onReady();
      } else {
        setError("Что-то из шагов не завершилось. Нажмите «Повторить».");
      }
    } catch (e) {
      setError(errorText(e));
      setActive(null);
      setPercent(null);
      // Часть шагов могла пройти — показываем актуальное состояние.
      setStatus(await setupStatus().catch(() => current));
    }
  }

  const running = active !== null;

  return (
    <div className="backdrop">
      <div className="modal wizard">
        <h2>Осталось доустановить нейросеть</h2>
        <p className="hint" style={{ margin: 0 }}>
          «Засечка» работает полностью на вашем компьютере — фотографии и тексты никуда не
          отправляются. Для этого нужно один раз скачать движок и модель. Дальше приложение
          запускается сразу.
        </p>

        <div className="steps">
          {STEPS.map((step, i) => {
            const state: StepState = active === step.id ? "running" : stateOf(step.id, status);
            return (
              <div className={`wstep ${state}`} key={step.id}>
                <span className="wstep-mark">{state === "done" ? "✓" : i + 1}</span>
                <div className="wstep-body">
                  <b>{step.title}</b>
                  <span className="hint">{step.note}</span>
                  {state === "running" && (
                    <>
                      <span className="hint">{message || "Работаю…"}</span>
                      <div className="progress">
                        <div
                          className={`progress-fill${percent === null ? " indeterminate" : ""}`}
                          style={percent === null ? undefined : { width: `${percent}%` }}
                        />
                      </div>
                    </>
                  )}
                </div>
              </div>
            );
          })}
        </div>

        {!status.winget_available && !status.ollama_installed && (
          <p className="hint" style={{ margin: 0 }}>
            В системе не нашёлся winget, поэтому установщик Ollama откроется обычным окном —
            пройдите его шаги и вернитесь сюда.
          </p>
        )}

        {error && <div className="banner">{error}</div>}

        <div className="actions">
          <button onClick={onSkip} disabled={running}>
            Позже
          </button>
          <button className="primary" onClick={run} disabled={running}>
            {running ? "Устанавливаю…" : error ? "Повторить" : "Установить и скачать"}
          </button>
        </div>

        <p className="hint" style={{ margin: 0, textAlign: "center" }}>
          Загрузка занимает 10–30 минут на обычном домашнем интернете. Окно можно свернуть.
        </p>
      </div>
    </div>
  );
}
