import { useState } from "react";

import { addPhotoBytes, addPhotos, errorText, pickPhotos, removePhoto } from "../api";
import type { PhotoInfo } from "../types";

interface Props {
  photos: PhotoInfo[];
  onChange: (photos: PhotoInfo[]) => void;
  onError: (message: string) => void;
  disabled: boolean;
}

export default function PhotoPanel({ photos, onChange, onError, disabled }: Props) {
  const [over, setOver] = useState(false);
  const [loading, setLoading] = useState(false);

  async function pick() {
    try {
      const paths = await pickPhotos();
      if (paths.length === 0) return;
      setLoading(true);
      onChange([...photos, ...(await addPhotos(paths))]);
    } catch (e) {
      onError(errorText(e));
    } finally {
      setLoading(false);
    }
  }

  /** Файлы, брошенные в окно, приходят как File — путь браузер не отдаёт. */
  async function drop(event: React.DragEvent) {
    event.preventDefault();
    setOver(false);
    if (disabled) return;

    const files = Array.from(event.dataTransfer.files).filter((f) => f.type.startsWith("image/"));
    if (files.length === 0) return;

    setLoading(true);
    const added: PhotoInfo[] = [];
    try {
      for (const file of files) {
        const bytes = new Uint8Array(await file.arrayBuffer());
        added.push(await addPhotoBytes(file.name, bytes));
      }
      onChange([...photos, ...added]);
    } catch (e) {
      onError(errorText(e));
    } finally {
      setLoading(false);
    }
  }

  async function remove(id: string) {
    await removePhoto(id);
    onChange(photos.filter((p) => p.id !== id));
  }

  return (
    <section className="section">
      <header>
        <span className="step">1</span>
        <h2>Фото товара</h2>
        <div className="spacer" />
        <button className="ghost" onClick={pick} disabled={disabled || loading}>
          Выбрать файлы
        </button>
      </header>

      <div
        className={`dropzone${over ? " over" : ""}`}
        onDragOver={(e) => {
          e.preventDefault();
          setOver(true);
        }}
        onDragLeave={() => setOver(false)}
        onDrop={drop}
      >
        {loading
          ? "Обрабатываю фото…"
          : "Перетащите сюда фотографии товара или нажмите «Выбрать файлы»"}
      </div>

      {photos.length > 0 && (
        <div className="thumbs">
          {photos.map((p) => (
            <div className="thumb" key={p.id} title={`${p.file_name} · ${p.width}×${p.height}`}>
              <img src={p.preview} alt={p.file_name} />
              <button className="ghost" onClick={() => remove(p.id)} disabled={disabled}>
                ✕
              </button>
            </div>
          ))}
        </div>
      )}

      <p className="hint" style={{ marginBottom: 0 }}>
        Достаточно 1–3 кадров: общий план, шильдик или этикетка и место с дефектом. Больше фото —
        дольше распознавание.
      </p>
    </section>
  );
}
