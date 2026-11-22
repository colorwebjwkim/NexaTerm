use crate::{
    backup,
    models::{Profile, Settings, View},
    ssh::{self, Control, Event, Runtime},
    store::Store,
};
use serde::Serialize;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{State, WebviewWindow, ipc::Channel};
use zeroize::Zeroizing;

pub struct PendingRestore {
    token: String,
    json: Zeroizing<Vec<u8>>,
    created: Instant,
}
pub struct AppState {
    pub store: Arc<Store>,
    pub runtime: Arc<Runtime>,
    pending: Arc<Mutex<Option<PendingRestore>>>,
    operation: tokio::sync::Mutex<()>,
    heavy: tokio::sync::Semaphore,
    pub closing: std::sync::atomic::AtomicBool,
}
impl AppState {
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(store),
            runtime: Arc::new(Runtime::default()),
            pending: Arc::new(Mutex::new(None)),
            operation: tokio::sync::Mutex::new(()),
            heavy: tokio::sync::Semaphore::new(2),
            closing: std::sync::atomic::AtomicBool::new(false),
        }
    }
    pub fn close_all(&self) {
        self.runtime.close_all();
        if let Ok(mut pending) = self.pending.lock() {
            pending.take();
        }
    }
}
pub fn allowed_url(url: &tauri::Url) -> bool {
    let packaged = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost"));
    let dev = cfg!(debug_assertions)
        && url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && url.port() == Some(1420);
    (packaged || dev)
        && ["/", "/index.html"].contains(&url.path())
        && url.query().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && (dev || url.port().is_none())
}
pub(crate) fn authorize(window: &WebviewWindow) -> Result<(), String> {
    let url = window.url().map_err(|_| "호출자 확인 실패")?;
    if window.label() != "main" || !allowed_url(&url) {
        return Err("허용되지 않은 호출자".into());
    }
    Ok(())
}
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "작업 실행 실패")?
}

#[tauri::command]
pub async fn get_state(window: WebviewWindow, state: State<'_, AppState>) -> Result<View, String> {
    authorize(&window)?;
    let store = state.store.clone();
    blocking(move || Ok(store.read()?.view())).await
}
#[tauri::command]
pub async fn save_profile(
    window: WebviewWindow,
    state: State<'_, AppState>,
    profile: Profile,
    password: Option<String>,
    save_secret: bool,
) -> Result<View, String> {
    authorize(&window)?;
    let store = state.store.clone();
    let password = password.map(Zeroizing::new);
    blocking(move || Ok(store.save_profile(profile, password, save_secret)?.view())).await
}
#[tauri::command]
pub async fn delete_profile(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<View, String> {
    authorize(&window)?;
    let store = state.store.clone();
    blocking(move || Ok(store.delete_profile(&id)?.view())).await
}
#[tauri::command]
pub async fn save_settings(
    window: WebviewWindow,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<View, String> {
    authorize(&window)?;
    let store = state.store.clone();
    blocking(move || Ok(store.save_settings(settings)?.view())).await
}
#[tauri::command]
pub async fn choose_private_key(window: WebviewWindow) -> Result<Option<String>, String> {
    authorize(&window)?;
    Ok(rfd::AsyncFileDialog::new()
        .set_title("SSH 개인키 선택")
        .pick_file()
        .await
        .map(|f| f.path().to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn export_backup(
    window: WebviewWindow,
    state: State<'_, AppState>,
    password: String,
    include_secrets: bool,
) -> Result<Option<String>, String> {
    authorize(&window)?;
    let password = Zeroizing::new(password);
    let store = state.store.clone();
    let _permit = state
        .heavy
        .try_acquire()
        .map_err(|_| "백업 작업이 진행 중입니다")?;
    let bytes =
        blocking(move || backup::encode(&store.read()?, &password, include_secrets)).await?;
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("암호화 설정 백업 저장")
        .set_file_name("NexaTerm-backup.nxtbackup")
        .add_filter("NexaTerm backup", &["nxtbackup"])
        .save_file()
        .await
    else {
        return Ok(None);
    };
    let path = file.path().to_path_buf();
    let label = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    blocking(move || backup::write_file(&path, &bytes)).await?;
    Ok(Some(label))
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    token: String,
    profiles: Vec<String>,
    password_count: usize,
    has_settings: bool,
}
#[tauri::command]
pub async fn preview_backup(
    window: WebviewWindow,
    state: State<'_, AppState>,
    password: String,
) -> Result<Option<Preview>, String> {
    authorize(&window)?;
    let password = Zeroizing::new(password);
    let _permit = state
        .heavy
        .try_acquire()
        .map_err(|_| "백업 작업이 진행 중입니다")?;
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("암호화 백업 선택")
        .add_filter("NexaTerm backup", &["nxtbackup"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let path = file.path().to_path_buf();
    let json = blocking(move || backup::read_file(&path, &password)).await?;
    let mut payload: backup::Backup =
        serde_json::from_slice(&json).map_err(|_| "백업 형식 오류")?;
    let token = uuid::Uuid::new_v4().to_string();
    let preview = Preview {
        token: token.clone(),
        profiles: payload
            .snapshot
            .profiles
            .iter()
            .map(|p| p.name.clone())
            .collect(),
        password_count: payload.passwords.len(),
        has_settings: true,
    };
    for password in payload.passwords.values_mut() {
        zeroize::Zeroize::zeroize(password);
    }
    *state.pending.lock().map_err(|_| "복원 잠금 오류")? = Some(PendingRestore {
        token: token.clone(),
        json,
        created: Instant::now(),
    });
    let slot = state.pending.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(300)).await;
        if let Ok(mut slot) = slot.lock()
            && slot.as_ref().is_some_and(|p| p.token == token)
        {
            slot.take();
        }
    });
    Ok(Some(preview))
}
#[tauri::command]
pub async fn restore_backup(
    window: WebviewWindow,
    state: State<'_, AppState>,
    token: String,
    mode: String,
    confirm_replace: bool,
) -> Result<View, String> {
    authorize(&window)?;
    let _operation = state.operation.lock().await;
    if !state
        .runtime
        .sessions
        .lock()
        .map_err(|_| "연결 잠금 오류")?
        .is_empty()
    {
        return Err("모든 연결을 종료한 후 복원하세요".into());
    }
    if mode == "replace" && !confirm_replace {
        return Err("전체 교체 확인이 필요합니다".into());
    }
    let pending = {
        let mut slot = state.pending.lock().map_err(|_| "복원 잠금 오류")?;
        let pending = slot.as_ref().ok_or("복원 미리보기가 없습니다")?;
        if pending.token != token || pending.created.elapsed() > Duration::from_secs(300) {
            return Err("복원 미리보기가 만료되었습니다".into());
        }
        slot.take().unwrap()
    };
    let store = state.store.clone();
    blocking(move || Ok(backup::restore(&store, &pending.json, &mode)?.view())).await
}
#[tauri::command]
pub fn cancel_restore(window: WebviewWindow, state: State<'_, AppState>) -> Result<(), String> {
    authorize(&window)?;
    state.pending.lock().map_err(|_| "복원 잠금 오류")?.take();
    Ok(())
}

#[tauri::command]
pub async fn connect_session(
    window: WebviewWindow,
    state: State<'_, AppState>,
    profile_id: String,
    password: Option<String>,
    events: Channel<Event>,
) -> Result<String, String> {
    authorize(&window)?;
    let store = state.store.clone();
    let runtime = state.runtime.clone();
    let password = password.map(Zeroizing::new);
    let _operation = state.operation.lock().await;
    let connection_store = store.clone();
    let (profile, secret) = blocking(move || {
        let doc = store.read()?;
        let profile = doc
            .profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or("프로필을 찾을 수 없습니다")?;
        let secret = match password {
            Some(p) if !p.is_empty() => p,
            _ if profile.secret_ref.is_some() => store.password(&profile)?,
            _ if profile.auth_method == "privateKey" => Zeroizing::new(String::new()),
            _ => return Err("비밀번호를 입력하세요".into()),
        };
        if secret.len() > 16384 {
            return Err("비밀번호 크기 제한".into());
        }
        Ok((profile, secret))
    })
    .await?;
    ssh::start(connection_store, runtime, profile, secret, events)
}
#[tauri::command]
pub fn terminal_input(
    window: WebviewWindow,
    state: State<'_, AppState>,
    connection_id: String,
    data: String,
) -> Result<(), String> {
    authorize(&window)?;
    if data.len() > 65536 {
        return Err("한 번에 입력할 수 있는 크기를 초과했습니다".into());
    }
    state
        .runtime
        .send(&connection_id, Control::Input(data.into_bytes()))
}
#[tauri::command]
pub fn terminal_resize(
    window: WebviewWindow,
    state: State<'_, AppState>,
    connection_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    authorize(&window)?;
    if !(2..=1000).contains(&cols) || !(1..=500).contains(&rows) {
        return Err("터미널 크기 오류".into());
    }
    state
        .runtime
        .send(&connection_id, Control::Resize(cols, rows))
}
#[tauri::command]
pub fn terminal_ack(
    window: WebviewWindow,
    state: State<'_, AppState>,
    connection_id: String,
    sequence: u64,
) -> Result<(), String> {
    authorize(&window)?;
    state.runtime.send(&connection_id, Control::Ack(sequence))
}
#[tauri::command]
pub async fn disconnect_session(
    window: WebviewWindow,
    state: State<'_, AppState>,
    connection_id: String,
    transfer_policy: Option<String>,
) -> Result<(), String> {
    authorize(&window)?;
    let files = &state.runtime.files;
    if files.active(&connection_id) > 0 {
        match transfer_policy.as_deref() {
            Some("keep") => {
                state.runtime.detach(&connection_id)?;
                if files.active(&connection_id) == 0 {
                    state.runtime.close(&connection_id);
                }
                return Ok(());
            }
            Some("cancel") => {
                files.cancel_connection(&connection_id);
                for _ in 0..100 {
                    if files.active(&connection_id) == 0 {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
            _ => {
                return Err(
                    "진행 중인 전송이 있습니다. 전송 유지 또는 취소 후 닫기를 선택하세요".into(),
                );
            }
        }
    }
    state.runtime.close(&connection_id);
    Ok(())
}
#[tauri::command]
pub fn resolve_host_key(
    window: WebviewWindow,
    state: State<'_, AppState>,
    token: String,
    approve: bool,
    approve_changed: bool,
) -> Result<(), String> {
    authorize(&window)?;
    let mut approvals = state
        .runtime
        .approvals
        .lock()
        .map_err(|_| "호스트키 잠금 오류")?;
    let approval = approvals
        .get(&token)
        .ok_or("호스트키 확인 요청이 만료되었습니다")?;
    if approve && approval.changed && !approve_changed {
        return Err("변경된 호스트키 재승인이 필요합니다".into());
    }
    let approval = approvals.remove(&token).unwrap();
    approval
        .sender
        .send(approve)
        .map_err(|_| "호스트키 확인 요청이 종료되었습니다".into())
}
