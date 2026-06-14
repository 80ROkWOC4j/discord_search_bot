#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{AppHandle, Manager};

const DISCORD_USER_ENDPOINT: &str = "https://discord.com/api/v10/users/@me";
const DISCORD_INVITE_PERMISSIONS: &str = "19327372288";
const DB_KEY_RELATIVE_PATH: &[&str] = &["secrets", "db_key"];
const PLAIN_DB_NAME: &str = "discord_bot.db";
const PLAIN_DB_WAL_NAME: &str = "discord_bot.db-wal";
const PLAIN_DB_SHM_NAME: &str = "discord_bot.db-shm";
const ENCRYPTED_DB_NAME: &str = "discord_bot.sqlcipher.db";
const ENCRYPTED_DB_WAL_NAME: &str = "discord_bot.sqlcipher.db-wal";
const ENCRYPTED_DB_SHM_NAME: &str = "discord_bot.sqlcipher.db-shm";
const EMBEDDED_BOT_EXE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/release/discord_search_bot.exe"
));

#[derive(Default)]
struct LauncherState {
    child: Mutex<Option<Child>>,
    logs: Arc<Mutex<Vec<String>>>,
}

impl Drop for LauncherState {
    fn drop(&mut self) {
        let Ok(child_slot) = self.child.get_mut() else {
            return;
        };
        if let Some(mut child) = child_slot.take() {
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
struct LauncherConfig {
    token: String,
    client_id: String,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            client_id: String::new(),
        }
    }
}

#[derive(Serialize)]
struct BotStatus {
    running: bool,
    message: String,
}

#[derive(Deserialize, Serialize)]
struct DiscordUser {
    id: String,
    username: String,
}

#[tauri::command]
async fn validate_token(token: String) -> Result<DiscordUser, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("봇 토큰을 입력하세요.".to_string());
    }

    let client = reqwest::Client::new();
    let response = client
        .get(DISCORD_USER_ENDPOINT)
        .header(AUTHORIZATION, format!("Bot {token}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("토큰 확인 실패: {}", response.status()));
    }

    response
        .json::<DiscordUser>()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn invite_url(client_id: String) -> Result<String, String> {
    let client_id = client_id.trim();
    if client_id.is_empty() {
        return Err("Client ID를 입력하세요.".to_string());
    }
    if !client_id.chars().all(|c| c.is_ascii_digit()) {
        return Err("Client ID는 숫자만 입력하세요.".to_string());
    }

    Ok(format!(
        "https://discord.com/oauth2/authorize?client_id={client_id}&permissions={DISCORD_INVITE_PERMISSIONS}&scope=bot%20applications.commands"
    ))
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    open_url_with_system(&url)
}

#[tauri::command]
fn open_data_dir(app: AppHandle) -> Result<(), String> {
    let path = default_data_dir(&app)?;
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    open_path(&path)
}

#[tauri::command]
fn start_bot(
    app: AppHandle,
    state: tauri::State<'_, LauncherState>,
    config: LauncherConfig,
) -> Result<BotStatus, String> {
    let token = config.token.trim();
    if token.is_empty() {
        return Err("봇 토큰을 입력하세요.".to_string());
    }

    let data_dir = default_data_dir(&app)?;
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    ensure_database_key(&data_dir, &state.logs)?;

    let bot_path = resolve_bot_exe(&app)?;
    let mut child_slot = state.child.lock().map_err(|e| e.to_string())?;
    if is_child_running(&mut child_slot)? {
        return Ok(BotStatus {
            running: true,
            message: "이미 실행 중".to_string(),
        });
    }

    push_log(&state.logs, format!("Starting bot: {}", bot_path.display()));
    push_log(
        &state.logs,
        format!("Data directory: {}", data_dir.display()),
    );

    let mut command = Command::new(&bot_path);
    command
        .current_dir(&data_dir)
        .env("DISCORD_TOKEN", token)
        .env("DATABASE_URL", "sqlite://discord_bot.db?mode=rwc")
        .env("VERSION_CHECK_INTERVAL_SECS", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }

    let mut child = command.spawn().map_err(|e| e.to_string())?;
    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(stdout, state.logs.clone(), "bot");
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(stderr, state.logs.clone(), "bot");
    }

    *child_slot = Some(child);
    Ok(BotStatus {
        running: true,
        message: "실행 중".to_string(),
    })
}

#[tauri::command]
fn stop_bot(state: tauri::State<'_, LauncherState>) -> Result<BotStatus, String> {
    stop_running_bot(&state)?;

    Ok(BotStatus {
        running: false,
        message: "중지됨".to_string(),
    })
}

#[tauri::command]
fn clear_local_data(
    app: AppHandle,
    state: tauri::State<'_, LauncherState>,
) -> Result<BotStatus, String> {
    stop_running_bot(&state)?;

    let data_dir = default_data_dir(&app)?;
    for name in [
        PLAIN_DB_NAME,
        PLAIN_DB_WAL_NAME,
        PLAIN_DB_SHM_NAME,
        ENCRYPTED_DB_NAME,
        ENCRYPTED_DB_WAL_NAME,
        ENCRYPTED_DB_SHM_NAME,
    ] {
        remove_file_if_exists(&data_dir.join(name))?;
    }
    remove_dir_if_exists(&data_dir.join("logs"))?;
    remove_dir_if_exists(&data_dir.join("secrets"))?;

    if let Ok(mut lines) = state.logs.lock() {
        lines.clear();
    }

    Ok(BotStatus {
        running: false,
        message: "로컬 데이터 삭제됨".to_string(),
    })
}

fn stop_running_bot(state: &LauncherState) -> Result<bool, String> {
    let mut child_slot = state.child.lock().map_err(|e| e.to_string())?;
    if let Some(child) = child_slot.as_mut()
        && child.try_wait().map_err(|e| e.to_string())?.is_some()
    {
        *child_slot = None;
        return Ok(false);
    }

    if let Some(mut child) = child_slot.take() {
        child.kill().map_err(|e| e.to_string())?;
        let _ = child.wait();
        push_log(&state.logs, "Bot stopped".to_string());
        return Ok(true);
    }

    Ok(false)
}

#[tauri::command]
fn bot_status(state: tauri::State<'_, LauncherState>) -> Result<BotStatus, String> {
    let mut child_slot = state.child.lock().map_err(|e| e.to_string())?;
    let running = is_child_running(&mut child_slot)?;

    Ok(BotStatus {
        running,
        message: if running {
            "실행 중".to_string()
        } else {
            "대기 중".to_string()
        },
    })
}

#[tauri::command]
fn read_logs(state: tauri::State<'_, LauncherState>) -> Result<Vec<String>, String> {
    state
        .logs
        .lock()
        .map(|lines| lines.clone())
        .map_err(|e| e.to_string())
}

fn default_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?)
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("{}: {}", path.display(), error)),
    }
}

fn remove_dir_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("{}: {}", path.display(), error)),
    }
}

fn resolve_bot_exe(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("DSB_BOT_EXE").map(PathBuf::from)
        && path.exists()
    {
        return Ok(path);
    }

    let exe_name = if cfg!(windows) {
        "discord_search_bot.exe"
    } else {
        "discord_search_bot"
    };

    let mut candidates = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join(exe_name));
    }
    if let Ok(current_exe) = std::env::current_exe()
        && let Some(current_dir) = current_exe.parent()
    {
        candidates.push(current_dir.join(exe_name));
        candidates.push(
            current_dir
                .join("..")
                .join("..")
                .join("..")
                .join("..")
                .join("target")
                .join("debug")
                .join(exe_name),
        );
        candidates.push(
            current_dir
                .join("..")
                .join("..")
                .join("..")
                .join("..")
                .join("target")
                .join("release")
                .join(exe_name),
        );
    }

    if let Some(path) = candidates.into_iter().find(|candidate| candidate.exists()) {
        return Ok(path);
    }

    extract_embedded_bot(app, exe_name)
}

fn extract_embedded_bot(app: &AppHandle, exe_name: &str) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_cache_dir()
        .or_else(|_| app.path().app_data_dir())
        .map_err(|e| e.to_string())?
        .join("bin");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let path = dir.join(exe_name);
    let should_write = fs::metadata(&path)
        .map(|metadata| metadata.len() != EMBEDDED_BOT_EXE.len() as u64)
        .unwrap_or(true);

    if should_write {
        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, EMBEDDED_BOT_EXE).map_err(|e| e.to_string())?;
        if path.exists() {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        fs::rename(temp_path, &path).map_err(|e| e.to_string())?;
    }

    Ok(path)
}

fn ensure_database_key(data_dir: &Path, logs: &Arc<Mutex<Vec<String>>>) -> Result<(), String> {
    let key_path = DB_KEY_RELATIVE_PATH
        .iter()
        .fold(data_dir.to_path_buf(), |path, part| path.join(part));
    if key_path.exists() {
        return Ok(());
    }

    let plaintext_db = data_dir.join(PLAIN_DB_NAME);
    let encrypted_db = data_dir.join(ENCRYPTED_DB_NAME);
    if plaintext_db.exists() || encrypted_db.exists() {
        push_log(
            logs,
            "Existing database found; database encryption key was not created automatically."
                .to_string(),
        );
        return Ok(());
    }

    let key_dir = key_path
        .parent()
        .ok_or_else(|| "Invalid database key path".to_string())?;
    fs::create_dir_all(key_dir).map_err(|e| e.to_string())?;

    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&key_path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };

    let key = generate_database_key()?;
    file.write_all(key.as_bytes()).map_err(|e| e.to_string())?;
    file.write_all(b"\n").map_err(|e| e.to_string())?;
    push_log(
        logs,
        format!("Database encryption key created: {}", key_path.display()),
    );
    Ok(())
}

fn generate_database_key() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    fill_random_bytes(&mut bytes)?;
    Ok(hex_encode(&bytes))
}

#[cfg(windows)]
fn fill_random_bytes(bytes: &mut [u8]) -> Result<(), String> {
    #[link(name = "advapi32")]
    unsafe extern "system" {
        #[link_name = "SystemFunction036"]
        fn rtl_gen_random(buffer: *mut u8, length: u32) -> u8;
    }

    let length = u32::try_from(bytes.len()).map_err(|_| "Random buffer too large".to_string())?;
    let ok = unsafe { rtl_gen_random(bytes.as_mut_ptr(), length) };
    if ok == 0 {
        return Err("Windows random generator failed".to_string());
    }

    Ok(())
}

#[cfg(not(windows))]
fn fill_random_bytes(bytes: &mut [u8]) -> Result<(), String> {
    let mut file = fs::File::open("/dev/urandom").map_err(|e| e.to_string())?;
    file.read_exact(bytes).map_err(|e| e.to_string())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);

    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }

    output
}

fn is_child_running(child_slot: &mut Option<Child>) -> Result<bool, String> {
    let exited = match child_slot.as_mut() {
        Some(child) => child.try_wait().map_err(|e| e.to_string())?.is_some(),
        None => return Ok(false),
    };

    if exited {
        *child_slot = None;
        Ok(false)
    } else {
        Ok(true)
    }
}

fn spawn_log_reader<R>(reader: R, logs: Arc<Mutex<Vec<String>>>, prefix: &'static str)
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines().map_while(Result::ok) {
            push_log(&logs, format!("[{prefix}] {line}"));
        }
    });
}

fn push_log(logs: &Arc<Mutex<Vec<String>>>, line: String) {
    if let Ok(mut lines) = logs.lock() {
        lines.push(line);
        let overflow = lines.len().saturating_sub(500);
        if overflow > 0 {
            lines.drain(0..overflow);
        }
    }
}

fn open_url_with_system(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler").arg(url);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    command.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("explorer.exe");
        command.arg(path);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    command.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .manage(LauncherState::default())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let state = window.state::<LauncherState>();
                let _ = stop_running_bot(&state);
            }
        })
        .invoke_handler(tauri::generate_handler![
            validate_token,
            invite_url,
            open_url,
            open_data_dir,
            start_bot,
            stop_bot,
            clear_local_data,
            bot_status,
            read_logs
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
