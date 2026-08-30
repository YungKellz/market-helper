//! Мастер первого запуска: доводит машину пользователя до состояния, в котором
//! приложение может генерировать. Ставит Ollama, поднимает её и качает модель.
//!
//! Сознательно не тащим движок и веса внутрь установщика: дистрибутив остался бы
//! на 6–8 ГБ, а обновлять вшитую модель пришлось бы новой сборкой приложения.

use std::path::PathBuf;
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::config::{AppConfig, BackendKind};
use crate::error::{AppError, AppResult};
use crate::llm::LlmService;

/// Событие с ходом установки.
pub const EVENT_SETUP: &str = "setup:progress";

/// Официальный установщик Ollama. Используется, только если в системе почему-то
/// нет winget.
const OLLAMA_INSTALLER_URL: &str = "https://ollama.com/download/OllamaSetup.exe";

#[derive(Debug, Clone, Serialize)]
pub struct SetupStatus {
    /// Бинарник найден на диске — даже если сервис сейчас не поднят.
    pub ollama_installed: bool,
    /// Отвечает по HTTP на 11434.
    pub ollama_running: bool,
    pub model_ready: bool,
    pub model: String,
    /// Есть ли winget: с ним установка тихая, без него — с окном инсталлятора.
    pub winget_available: bool,
    /// Показывать ли мастер при запуске.
    pub needs_setup: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetupProgress {
    /// `install` | `start` | `model`
    pub step: &'static str,
    pub message: String,
    pub percent: Option<u8>,
    pub done: bool,
}

fn emit(app: &AppHandle, step: &'static str, message: impl Into<String>, percent: Option<u8>) {
    let _ = app.emit(
        EVENT_SETUP,
        SetupProgress { step, message: message.into(), percent, done: false },
    );
}

fn emit_done(app: &AppHandle, step: &'static str, message: impl Into<String>) {
    let _ = app.emit(
        EVENT_SETUP,
        SetupProgress { step, message: message.into(), percent: Some(100), done: true },
    );
}

fn find_in_path(exe: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(exe))
        .find(|candidate| candidate.exists())
}

/// Официальный установщик кладёт Ollama в профиль пользователя и не всегда
/// успевает обновить PATH текущего процесса — проверяем оба места.
fn ollama_binary() -> Option<PathBuf> {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let candidate = PathBuf::from(local)
            .join("Programs")
            .join("Ollama")
            .join("ollama.exe");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    find_in_path("ollama.exe")
}

fn winget_binary() -> Option<PathBuf> {
    find_in_path("winget.exe")
}

pub async fn status(llm: &LlmService, cfg: &AppConfig) -> SetupStatus {
    let ollama = llm.ollama(cfg).status().await;
    let installed = ollama_binary().is_some();
    let model_ready = ollama.vision_model_ready && ollama.text_model_ready;

    SetupStatus {
        ollama_installed: installed,
        ollama_running: ollama.available,
        model_ready,
        model: cfg.ollama.vision_model.clone(),
        winget_available: winget_binary().is_some(),
        // Встроенный llama-server настраивается руками в «Настройках»: если
        // пользователь выбрал его явно, Ollama ему не нужна и мастер молчит.
        needs_setup: cfg.backend != BackendKind::LlamaCpp && !(ollama.available && model_ready),
    }
}

#[cfg(windows)]
fn hide_console(cmd: &mut tokio::process::Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_cmd: &mut tokio::process::Command) {}

/// Установка через winget — штатный менеджер пакетов Windows 11. Ollama ставится
/// в профиль пользователя, права администратора не нужны.
async fn install_via_winget(app: &AppHandle, winget: PathBuf) -> AppResult<()> {
    emit(app, "install", "Устанавливаю Ollama через winget…", None);

    let mut cmd = tokio::process::Command::new(winget);
    cmd.args([
        "install",
        "--id",
        "Ollama.Ollama",
        "--exact",
        "--silent",
        "--accept-package-agreements",
        "--accept-source-agreements",
        "--disable-interactivity",
    ])
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::null());
    hide_console(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::Other(format!("не удалось запустить winget: {e}")))?;

    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            // winget рисует прогресс-бар возвратом каретки: берём последний
            // фрагмент строки и отбрасываем псевдографику.
            let last = line.rsplit('\r').next().unwrap_or(&line).trim();
            if !last.is_empty() && !last.starts_with(['█', '▒', '-', '\\', '|', '/']) {
                emit(app, "install", last, None);
            }
        }
    }

    let status = child.wait().await?;
    if !status.success() {
        return Err(AppError::Other(format!(
            "winget завершился с кодом {}. Попробуйте установить Ollama вручную с ollama.com",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

/// Запасной путь: качаем официальный установщик и открываем его. Тихо ставить
/// скачанный exe не пытаемся — пользователь должен видеть, что именно ставится.
async fn install_via_download(app: &AppHandle) -> AppResult<()> {
    emit(app, "install", "Скачиваю установщик Ollama…", Some(0));

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(1800))
        .build()
        .map_err(|e| AppError::Other(e.to_string()))?;

    let resp = http.get(OLLAMA_INSTALLER_URL).send().await?;
    if !resp.status().is_success() {
        return Err(AppError::Other(format!(
            "не удалось скачать установщик Ollama: сервер ответил {}",
            resp.status()
        )));
    }

    let total = resp.content_length();
    let target = std::env::temp_dir().join("OllamaSetup.exe");
    let mut file = tokio::fs::File::create(&target).await?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        downloaded += chunk.len() as u64;
        file.write_all(&chunk).await?;
        if let Some(total) = total {
            let percent = ((downloaded as f64 / total as f64) * 100.0) as u8;
            emit(
                app,
                "install",
                format!("Скачиваю установщик Ollama… {} МБ", downloaded / 1_048_576),
                Some(percent),
            );
        }
    }
    file.flush().await?;
    drop(file);

    emit(
        app,
        "install",
        "Открываю установщик Ollama — пройдите его шаги и вернитесь сюда",
        Some(100),
    );
    std::process::Command::new(&target)
        .spawn()
        .map_err(|e| AppError::Other(format!("не удалось запустить установщик: {e}")))?;
    Ok(())
}

pub async fn install_ollama(app: &AppHandle) -> AppResult<()> {
    if ollama_binary().is_some() {
        emit_done(app, "install", "Ollama уже установлена");
        return Ok(());
    }

    match winget_binary() {
        Some(winget) => install_via_winget(app, winget).await?,
        None => {
            install_via_download(app).await?;
            // Установщик проходит пользователь, поэтому ждём появления бинарника.
            emit(app, "install", "Жду завершения установки…", None);
            for _ in 0..600 {
                if ollama_binary().is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }

    if ollama_binary().is_none() {
        return Err(AppError::Other(
            "установка завершилась, но ollama.exe не найден — перезапустите приложение".into(),
        ));
    }
    emit_done(app, "install", "Ollama установлена");
    Ok(())
}

/// Поднимаем сервис. Предпочитаем `ollama app.exe` — это фоновый трей-процесс,
/// который переживёт закрытие нашего приложения, как при обычной установке.
pub async fn start_ollama(app: &AppHandle, llm: &LlmService, cfg: &AppConfig) -> AppResult<()> {
    if llm.ollama(cfg).status().await.available {
        emit_done(app, "start", "Ollama уже запущена");
        return Ok(());
    }

    let binary = ollama_binary()
        .ok_or_else(|| AppError::Other("Ollama не установлена".into()))?;
    emit(app, "start", "Запускаю Ollama…", None);

    let tray = binary.with_file_name("ollama app.exe");
    let mut cmd = if tray.exists() {
        tokio::process::Command::new(tray)
    } else {
        let mut c = tokio::process::Command::new(&binary);
        c.arg("serve");
        c
    };
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    hide_console(&mut cmd);
    cmd.spawn()
        .map_err(|e| AppError::Other(format!("не удалось запустить Ollama: {e}")))?;

    for _ in 0..60 {
        if llm.ollama(cfg).status().await.available {
            emit_done(app, "start", "Ollama запущена");
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(AppError::Other(
        "Ollama не ответила за 30 секунд после запуска".into(),
    ))
}
