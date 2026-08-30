import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import type {
  AppConfig,
  BackendStatus,
  GenerateOptions,
  ListingDraft,
  ListingResult,
  PhotoInfo,
  ProductFacts,
  SetupProgress,
  SetupStatus,
  UserAttributes,
} from "./types";

export const getConfig = () => invoke<AppConfig>("get_config");

export const saveConfig = (config: AppConfig) => invoke<void>("save_config", { config });

export const backendStatus = () => invoke<BackendStatus[]>("backend_status");

export const pullModel = (model: string) => invoke<void>("pull_model", { model });

export const addPhotos = (paths: string[]) => invoke<PhotoInfo[]>("add_photos", { paths });

export const addPhotoBytes = (fileName: string, bytes: Uint8Array) =>
  invoke<PhotoInfo>("add_photo_bytes", { fileName, bytes: Array.from(bytes) });

export const removePhoto = (id: string) => invoke<void>("remove_photo", { id });

export const analyzePhotos = (photoIds: string[], hint: string) =>
  invoke<ProductFacts>("analyze_photos", { photoIds, hint });

export const generateListing = (
  facts: ProductFacts,
  attributes: UserAttributes,
  options: GenerateOptions,
) => invoke<ListingResult>("generate_listing", { facts, attributes, options });

export const refineListing = (
  draft: ListingDraft,
  instruction: string,
  options: GenerateOptions,
) => invoke<ListingResult>("refine_listing", { draft, instruction, options });

export const lintListing = (draft: ListingDraft) => invoke<ListingResult>("lint_listing", { draft });

/** Поток токенов генерации. Возвращает функцию отписки. */
export const onToken = (handler: (chunk: string) => void): Promise<UnlistenFn> =>
  listen<string>("generation:token", (e) => handler(e.payload));

/** Прогресс `ollama pull` — сырые NDJSON-строки. */
export const onPullProgress = (handler: (line: string) => void): Promise<UnlistenFn> =>
  listen<string>("model:pull", (e) => handler(e.payload));

/** Системный диалог выбора файлов. Авито принимает jpg, png и gif. */
export async function pickPhotos(): Promise<string[]> {
  const picked = await open({
    multiple: true,
    filters: [{ name: "Изображения", extensions: ["jpg", "jpeg", "png", "webp", "gif", "bmp"] }],
  });
  if (!picked) return [];
  return Array.isArray(picked) ? picked : [picked];
}

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

export const setupStatus = () => invoke<SetupStatus>("setup_status");

export const installOllama = () => invoke<void>("install_ollama");

export const startOllama = () => invoke<void>("start_ollama");

/** Ход мастера первого запуска. */
export const onSetupProgress = (handler: (p: SetupProgress) => void): Promise<UnlistenFn> =>
  listen<SetupProgress>("setup:progress", (e) => handler(e.payload));
