/**
 * Собирает подписанный релиз и выкладывает его на GitHub Releases.
 *
 * Один прогон делает всё, что нужно для обновления у коллег: собирает
 * установщик, подписывает артефакты обновления, переименовывает их в латиницу,
 * пишет latest.json и создаёт релиз.
 *
 * Почему Node, а не PowerShell. Ключ подписи создан без пароля, и Tauri
 * ждёт пустое значение в TAURI_SIGNING_PRIVATE_KEY_PASSWORD. PowerShell
 * удаляет переменную окружения при присвоении пустой строки, поэтому оттуда
 * пустой пароль передать нельзя вообще: сборка доходит до подписи и молча
 * виснет на приглашении ввода. Node пустое значение держит.
 *
 * Почему артефакты переименовываются. Продукт называется «Засечка», и Tauri
 * именует файлы кириллицей. GitHub заменяет в именах вложений всё, кроме
 * латиницы, цифр, точки, дефиса и подчёркивания, — ссылка в latest.json
 * перестала бы совпадать с настоящим адресом, и обновление молча не
 * находилось бы.
 *
 * Что именно скачивает обновлятор. В Tauri v2 артефакт обновления для NSIS —
 * это сам файл установщика и подпись рядом с ним, а не отдельный архив.
 * Поэтому в релиз уходят ровно два файла: установщик и latest.json.
 *
 * Запуск: pnpm release -- --notes "Что нового"
 * Проверка без выкладки: pnpm release -- --dry-run
 * Длинный текст: pnpm release -- --notes-file notes.md
 */
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, copyFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const KEY_PATH = join(homedir(), ".tauri", "zasechka-updater.key");
const SLUG = "zasechka";
const REPO = "YungKellz/market-helper";

function fail(message) {
  console.error("Ошибка: " + message);
  process.exit(1);
}

function arg(name) {
  const i = process.argv.indexOf("--" + name);
  return i === -1 ? null : process.argv[i + 1] ?? "";
}

const notesFileArg = arg("notes-file");
if (notesFileArg && !existsSync(notesFileArg)) {
  // Проверяем до сборки: обидно потратить несколько минут и упасть на опечатке
  // в пути.
  fail(`не найден файл с примечаниями: ${notesFileArg}`);
}
// Текст нужен в двух местах: на странице релиза и в манифесте, откуда его
// показывает полоса обновления внутри приложения.
const notes = notesFileArg ? readFileSync(notesFileArg, "utf8") : arg("notes");
const draft = process.argv.includes("--draft");

if (!existsSync(KEY_PATH)) {
  fail(
    `не найден приватный ключ подписи: ${KEY_PATH}\n` +
      "Без него уже установленные копии не примут обновление.",
  );
}

const conf = JSON.parse(readFileSync(join(ROOT, "src-tauri", "tauri.conf.json"), "utf8"));
const { version, productName } = conf;
const tag = `v${version}`;

console.log(`Релиз ${productName} ${version}`);

const existing = spawnSync("gh", ["release", "view", tag, "--json", "tagName"], {
  stdio: "ignore",
});
if (existing.status === 0) {
  fail(`релиз ${tag} уже существует. Поднимите version в src-tauri/tauri.conf.json.`);
}

const buildEnv = {
  ...process.env,
  TAURI_SIGNING_PRIVATE_KEY: readFileSync(KEY_PATH, "utf8").trim(),
  // Ключ без пароля. Значение обязано присутствовать и быть пустым, иначе
  // Tauri уходит в интерактивное приглашение и сборка виснет.
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "",
};

const build = spawnSync("pnpm tauri build", {
  cwd: ROOT,
  env: buildEnv,
  stdio: "inherit",
  shell: true,
});
if (build.status !== 0) fail("сборка не удалась");

const nsisDir = join(ROOT, "src-tauri", "target", "release", "bundle", "nsis");
const setup = join(nsisDir, `${productName}_${version}_x64-setup.exe`);
const signature = `${setup}.sig`;

for (const file of [setup, signature]) {
  if (!existsSync(file)) {
    fail(
      `не найден артефакт ${file}\n` +
        "Проверьте, что в tauri.conf.json включён bundle.createUpdaterArtifacts.",
    );
  }
}

const stage = join(ROOT, "src-tauri", "target", "release", "bundle", "release");
rmSync(stage, { recursive: true, force: true });
mkdirSync(stage, { recursive: true });

const setupName = `${SLUG}_${version}_x64-setup.exe`;
const setupOut = join(stage, setupName);
copyFileSync(setup, setupOut);

const manifest = {
  version,
  notes: notes ?? "",
  pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
  platforms: {
    "windows-x86_64": {
      signature: readFileSync(signature, "utf8").trim(),
      url: `https://github.com/${REPO}/releases/download/${tag}/${setupName}`,
    },
  },
};
const manifestPath = join(stage, "latest.json");
writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n", "utf8");

// Примечания уходят в gh файлом, а не аргументом: многострочный текст,
// проведённый через оболочку Windows, теряет переводы строк и кавычки.
let notesPath = notesFileArg;
if (!notesPath && notes) {
  notesPath = join(stage, "notes.md");
  writeFileSync(notesPath, notes.endsWith("\n") ? notes : notes + "\n", "utf8");
}

console.log("Собрано для выкладки:");
for (const file of [setupOut, manifestPath]) {
  console.log("  " + file.slice(stage.length + 1));
}

if (process.argv.includes("--dry-run")) {
  console.log("Пробный прогон: релиз не создаётся. Проверьте latest.json выше по пути.");
  process.exit(0);
}

const ghArgs = ["release", "create", tag, setupOut, manifestPath,
                "--title", `${productName} ${version}`];
if (notesPath) ghArgs.push("--notes-file", notesPath);
else ghArgs.push("--generate-notes");
if (draft) ghArgs.push("--draft");

try {
  // Без shell: иначе Windows-оболочка пересобирает аргументы и заголовок
  // с пробелом разваливается на несколько.
  execFileSync("gh", ghArgs, { stdio: "inherit" });
} catch {
  fail("не удалось создать релиз");
}

console.log("Готово. Установленные копии увидят обновление при следующем запуске.");
