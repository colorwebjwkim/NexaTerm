use crate::{
    commands::{AppState, authorize},
    sftp::{self, Listing, LocalRoot, Transfer, TransferSpec},
};
use tauri::{Manager, State, WebviewWindow};

#[derive(serde::Serialize)]
pub struct LocalLocation {
    pub root: LocalRoot,
    pub listing: Listing,
}

pub(crate) fn restore_local_location(
    files: &sftp::FileManager,
    store: &crate::store::Store,
    defaults: &[std::path::PathBuf],
) -> Result<LocalLocation, String> {
    if let Some((root_path, path)) = store.local_location()?
        && let Ok(root) = files.grant_root(std::path::Path::new(&root_path))
        && let Ok(listing) = files.local_list(&root.token, &path)
    {
        return Ok(LocalLocation { root, listing });
    }
    for default in defaults {
        if let Ok(root) = files.grant_root(default)
            && let Ok(listing) = files.local_list(&root.token, "")
        {
            store.save_local_location(&root.path, "")?;
            return Ok(LocalLocation { root, listing });
        }
    }
    Err("문서 폴더에 접근할 수 없습니다. 로컬 폴더를 선택하세요.".into())
}

#[tauri::command]
pub async fn open_local_location(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<LocalLocation, String> {
    authorize(&window)?;
    let mut defaults = Vec::new();
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("NEXATERM_TEST_LOCAL_ROOT") {
        defaults.push(std::path::PathBuf::from(path));
    }
    if let Ok(path) = window.app_handle().path().document_dir() {
        defaults.push(path);
    }
    if let Ok(path) = window.app_handle().path().home_dir() {
        defaults.push(path);
    }
    let files = state.runtime.files.clone();
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || restore_local_location(&files, &store, &defaults))
        .await
        .map_err(|_| "로컬 시작 폴더 작업 실패")?
}
#[cfg(windows)]
fn rename_local(source: &std::path::Path, target: &std::path::Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: paths are NUL-terminated owned UTF-16 arrays. MoveFileW refuses existing targets.
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileW(source.as_ptr(), target.as_ptr())
    } == 0
    {
        Err("로컬 이름 변경 실패: 대상 이름/권한을 확인하세요".into())
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
fn rename_local(source: &std::path::Path, target: &std::path::Path) -> Result<(), String> {
    std::fs::rename(source, target).map_err(|_| "로컬 이름 변경 실패".into())
}

#[tauri::command]
pub fn local_roots(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<LocalRoot>, String> {
    authorize(&window)?;
    Ok(state.runtime.files.roots())
}
#[tauri::command]
pub async fn choose_local_root(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Option<LocalRoot>, String> {
    authorize(&window)?;
    let Some(folder) = rfd::AsyncFileDialog::new()
        .set_title("파일 관리에 사용할 로컬 폴더 선택")
        .pick_folder()
        .await
    else {
        return Ok(None);
    };
    let files = state.runtime.files.clone();
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = files.grant_root(folder.path())?;
        files.local_list(&root.token, "")?;
        store.save_local_location(&root.path, "")?;
        Ok(root)
    })
    .await
    .map_err(|_| "폴더 작업 실패")?
    .map(Some)
}
#[tauri::command]
pub async fn local_list(
    window: WebviewWindow,
    state: State<'_, AppState>,
    root: String,
    path: String,
    remember: Option<bool>,
) -> Result<Listing, String> {
    authorize(&window)?;
    let files = state.runtime.files.clone();
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let listing = files.local_list(&root, &path)?;
        if remember.unwrap_or(false) {
            let root_path = files.local(&root, "", false)?;
            store.save_local_location(&root_path.to_string_lossy(), &listing.path)?;
        }
        Ok(listing)
    })
    .await
    .map_err(|_| "로컬 목록 작업 실패")?
}
#[tauri::command]
pub async fn remote_list(
    window: WebviewWindow,
    state: State<'_, AppState>,
    connection_id: String,
    path: String,
) -> Result<Listing, String> {
    authorize(&window)?;
    state
        .runtime
        .files
        .remote_list(&state.runtime, &connection_id, &path)
        .await
}
#[tauri::command]
pub fn transfer_queue(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<Transfer>, String> {
    authorize(&window)?;
    Ok(state.runtime.files.jobs())
}
#[tauri::command]
pub fn enqueue_transfer(
    window: WebviewWindow,
    state: State<'_, AppState>,
    spec: TransferSpec,
) -> Result<String, String> {
    authorize(&window)?;
    state.runtime.files.enqueue(state.runtime.clone(), spec)
}
#[tauri::command]
pub fn cancel_transfer(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    authorize(&window)?;
    state.runtime.files.cancel(&id)
}
#[tauri::command]
pub fn retry_transfer(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    authorize(&window)?;
    state.runtime.files.retry(state.runtime.clone(), &id)
}
#[tauri::command]
pub fn resolve_transfer_conflict(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    decision: String,
) -> Result<(), String> {
    authorize(&window)?;
    state.runtime.files.decide(&id, &decision)
}
#[tauri::command]
pub async fn reset_sftp(
    window: WebviewWindow,
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), String> {
    authorize(&window)?;
    if state.runtime.files.active(&connection_id) > 0 {
        return Err("전송이 진행 중이어서 SFTP를 재연결할 수 없습니다".into());
    }
    state.runtime.files.close(&connection_id).await;
    Ok(())
}
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Explicit typed IPC parameters; no generic filesystem bridge.
pub async fn file_operation(
    window: WebviewWindow,
    state: State<'_, AppState>,
    connection_id: String,
    side: String,
    root: String,
    directory: String,
    name: String,
    operation: String,
    new_name: String,
    mode: Option<u32>,
    confirmed: bool,
) -> Result<(), String> {
    authorize(&window)?;
    sftp::safe_name(&name)?;
    if operation == "rename" {
        sftp::safe_name(&new_name)?;
    }
    if operation == "delete" && !confirmed {
        return Err("삭제 확인이 필요합니다".into());
    }
    if state.runtime.files.active(&connection_id) > 0 {
        return Err("해당 연결의 전송을 마친 뒤 파일을 변경하세요".into());
    }
    let files = state.runtime.files.clone();
    if side == "local" {
        tauri::async_runtime::spawn_blocking(move || {
            let relative = if directory.is_empty() {
                name.clone()
            } else {
                format!("{directory}/{name}")
            };
            let path = files.local(&root, &relative, operation == "mkdir")?;
            match operation.as_str() {
                "mkdir" => std::fs::create_dir(path).map_err(|_| "로컬 폴더 생성 실패".to_string()),
                "rename" => {
                    let relative = if directory.is_empty() {
                        new_name
                    } else {
                        format!("{directory}/{new_name}")
                    };
                    let target = files.local(&root, &relative, true)?;
                    if target.exists() {
                        return Err("대상 이름이 이미 있습니다".into());
                    }
                    rename_local(&path, &target)
                }
                "delete" => if path.is_dir() {
                    std::fs::remove_dir(path)
                } else {
                    std::fs::remove_file(path)
                }
                .map_err(|_| "로컬 삭제 실패: 폴더는 비어 있어야 합니다".into()),
                _ => Err("지원하지 않는 로컬 작업".into()),
            }
        })
        .await
        .map_err(|_| "로컬 파일 작업 실패")?
    } else if side == "remote" {
        let listing = files
            .remote_list(&state.runtime, &connection_id, &directory)
            .await?;
        let session = files.session(&state.runtime, &connection_id).await?;
        let path = format!("{}/{name}", listing.path.trim_end_matches('/'));
        let result = match operation.as_str() {
            "mkdir" => session.create_dir(path).await,
            "rename" => {
                let target = format!("{}/{}", listing.path.trim_end_matches('/'), new_name);
                if session
                    .try_exists(&target)
                    .await
                    .map_err(|_| "원격 대상 확인 실패")?
                {
                    return Err("대상 이름이 이미 있습니다".into());
                }
                session.rename(path, target).await
            }
            "delete" => {
                let metadata = session
                    .symlink_metadata(&path)
                    .await
                    .map_err(|_| "원격 파일 확인 실패")?;
                if metadata.is_dir() && !metadata.is_symlink() {
                    session.remove_dir(path).await
                } else {
                    session.remove_file(path).await
                }
            }
            "chmod" => {
                let permissions = mode.ok_or("권한 값이 없습니다")?;
                if permissions > 0o777 {
                    return Err("권한은 000~777 범위만 허용합니다".into());
                }
                let metadata = session
                    .symlink_metadata(&path)
                    .await
                    .map_err(|_| "원격 파일 확인 실패")?;
                if metadata.is_symlink() {
                    return Err("링크 권한은 변경하지 않습니다".into());
                }
                session
                    .set_metadata(
                        path,
                        russh_sftp::protocol::FileAttributes {
                            permissions: Some(permissions),
                            ..Default::default()
                        },
                    )
                    .await
            }
            _ => return Err("지원하지 않는 원격 작업".into()),
        };
        result.map_err(|_| "원격 작업 실패: 권한/연결/폴더 내용을 확인하세요".into())
    } else {
        Err("파일 패널 구분 오류".into())
    }
}
