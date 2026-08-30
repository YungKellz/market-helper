/** Зеркало serde-структур из src-tauri. Ключи — snake_case, как их отдаёт Rust. */

export type BackendKind = "auto" | "ollama" | "llama_cpp";

export interface OllamaConfig {
  base_url: string;
  vision_model: string;
  text_model: string;
  keep_alive: string;
}

export interface LlamaCppConfig {
  server_binary: string | null;
  model_path: string | null;
  mmproj_path: string | null;
  port: number;
  gpu_layers: number;
  context_size: number;
}

export interface GenerationConfig {
  temperature: number;
  top_p: number;
  target_chars_max: number;
  target_chars_min: number;
  image_max_side: number;
  image_jpeg_quality: number;
}

export interface SellerProfile {
  kind: "private" | "shop";
  city: string;
  delivery: string;
  pickup: string;
  bargain: boolean;
}

export interface AppConfig {
  backend: BackendKind;
  ollama: OllamaConfig;
  llama_cpp: LlamaCppConfig;
  generation: GenerationConfig;
  seller: SellerProfile;
}

export interface BackendStatus {
  kind: string;
  available: boolean;
  version: string | null;
  endpoint: string;
  models: string[];
  vision_model_ready: boolean;
  text_model_ready: boolean;
  detail: string;
}

export interface PhotoInfo {
  id: string;
  file_name: string;
  preview: string;
  width: number;
  height: number;
}

export interface ProductFacts {
  category: string;
  product_type: string;
  brand: string;
  model: string;
  color: string;
  material: string;
  condition: string;
  defects: string[];
  features: string[];
  included: string[];
  size: string;
  visible_text: string[];
  confidence: number;
  uncertain: string[];
  questions: string[];
}

export interface CustomAttribute {
  name: string;
  value: string;
}

export interface UserAttributes {
  title_hint: string;
  brand: string;
  model: string;
  condition: string;
  price: string;
  size: string;
  color: string;
  included: string;
  defects: string;
  reason_for_sale: string;
  custom: CustomAttribute[];
  notes: string;
}

export interface GenerateOptions {
  tone: "friendly" | "business" | "concise";
  audience: string;
  include_cta: boolean;
  include_tags: boolean;
  disclose_defects: boolean;
}

export interface ListingDraft {
  title: string;
  hook: string;
  description: string;
  tags: string[];
}

export interface Issue {
  severity: "error" | "warning";
  field: string;
  message: string;
  excerpt: string | null;
  /** Готовая инструкция модели для кнопки «Исправить». */
  fix: string;
}

export interface ListingResult extends ListingDraft {
  title_chars: number;
  description_chars: number;
  issues: Issue[];
  /** Какой бэкенд обслужил запрос: `ollama` или `llama_cpp`. */
  backend: string;
}

export const emptyFacts = (): ProductFacts => ({
  category: "",
  product_type: "",
  brand: "",
  model: "",
  color: "",
  material: "",
  condition: "",
  defects: [],
  features: [],
  included: [],
  size: "",
  visible_text: [],
  confidence: 0,
  uncertain: [],
  questions: [],
});

export const emptyAttributes = (): UserAttributes => ({
  title_hint: "",
  brand: "",
  model: "",
  condition: "",
  price: "",
  size: "",
  color: "",
  included: "",
  defects: "",
  reason_for_sale: "",
  custom: [],
  notes: "",
});

export const defaultOptions = (): GenerateOptions => ({
  tone: "friendly",
  audience: "",
  include_cta: true,
  include_tags: true,
  disclose_defects: true,
});

export interface SetupStatus {
  ollama_installed: boolean;
  ollama_running: boolean;
  model_ready: boolean;
  model: string;
  winget_available: boolean;
  needs_setup: boolean;
}

export interface SetupProgress {
  /** `install` | `start` | `model` */
  step: string;
  message: string;
  percent: number | null;
  done: boolean;
}
