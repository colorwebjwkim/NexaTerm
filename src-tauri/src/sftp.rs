use crate::ssh::Runtime;
use russh_sftp::{
    client::SftpSession,
    protocol::{FileAttributes, OpenFlags},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub directory: bool,
    pub blocked: bool,
    pub size: u64,
    pub modified: u64,
    pub mode: Option<u32>,
}
#[derive(Clone, Serialize)]
pub struct Listing {
    pub path: String,
    pub entries: Vec<Entry>,
}
#[derive(Clone, Serialize)]
pub struct LocalRoot {
    pub token: String,
    pub path: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferSpec {
    pub connection_id: String,
    pub direction: String,
    pub local_root: String,
    pub local_dir: String,
    pub remote_dir: String,
    pub names: Vec<String>,
    pub collision: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transfer {
    pub id: String,
    pub connection_id: String,
    pub direction: String,
    pub name: String,
    pub status: String,
    pub bytes: u64,
    pub total: u64,
    pub files: u32,
    pub skipped: u32,
    pub speed: u64,
    pub message: String,
    pub conflict: Option<String>,
}
struct Job {
    view: Transfer,
    spec: TransferSpec,
    cancel: CancellationToken,
    decision: Option<tokio::sync::oneshot::Sender<String>>,
}
pub struct FileManager {
    sessions: tokio::sync::Mutex<HashMap<String, Arc<SftpSession>>>,
    roots: Mutex<HashMap<String, PathBuf>>,
    jobs: Mutex<HashMap<String, Job>>,
    permits: tokio::sync::Semaphore,
}
impl Default for FileManager {
    fn default() -> Self {
        Self {
            sessions: Default::default(),
            roots: Default::default(),
            jobs: Default::default(),
            permits: tokio::sync::Semaphore::new(2),
        }
    }
}
fn message(context: &str) -> String {
    format!("{context}: 권한, 연결 또는 경로를 확인하세요")
}
fn seconds(time: Result<SystemTime, std::io::Error>) -> u64 {
    time.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |t| t.as_secs())
}
pub fn safe_name(name: &str) -> Result<(), String> {
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if name.is_empty()
        || name.len() > 240
        || name == "."
        || name == ".."
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || c == '\u{fffd}' || "/\\:<>\"|?*".contains(c))
        || ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
        return Err("안전하게 전송할 수 없는 파일명입니다".into());
    }
    Ok(())
}
fn relative(path: &str) -> Result<Vec<&str>, String> {
    if path.len() > 4096 || path.starts_with('/') || path.contains('\\') {
        return Err("로컬 상대 경로 오류".into());
    }
    let mut parts = vec![];
    for part in path.split('/').filter(|x| !x.is_empty()) {
        safe_name(part)?;
        parts.push(part);
    }
    Ok(parts)
}
fn child(base: &str, name: &str) -> String {
    if base.is_empty() {
        name.into()
    } else {
        format!("{}/{name}", base.trim_end_matches('/'))
    }
}
fn remote_path(path: &str) -> Result<(), String> {
    if path.len() > 4096
        || path.contains(['\0', '\\', '\u{fffd}'])
        || path.split('/').any(|s| s == "..")
    {
        Err("원격 경로 오류".into())
    } else {
        Ok(())
    }
}
fn reparse(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
impl FileManager {
    pub fn grant_root(&self, path: &Path) -> Result<LocalRoot, String> {
        let metadata =
            std::fs::symlink_metadata(path).map_err(|_| message("로컬 폴더 확인 실패"))?;
        if !metadata.is_dir() || reparse(&metadata) {
            return Err("일반 로컬 폴더를 선택하세요 (링크 폴더 제외)".into());
        }
        let path = std::fs::canonicalize(path).map_err(|_| message("로컬 경로 확인 실패"))?;
        let mut roots = self.roots.lock().map_err(|_| "폴더 잠금 오류")?;
        if let Some((token, _)) = roots.iter().find(|(_, granted)| **granted == path) {
            return Ok(LocalRoot {
                token: token.clone(),
                path: path.to_string_lossy().into_owned(),
            });
        }
        if roots.len() >= 64 {
            return Err("선택한 로컬 폴더가 너무 많습니다. 앱을 다시 시작하세요".into());
        }
        let token = uuid::Uuid::new_v4().to_string();
        roots.insert(token.clone(), path.clone());
        Ok(LocalRoot {
            token,
            path: path.to_string_lossy().into_owned(),
        })
    }
    pub fn roots(&self) -> Vec<LocalRoot> {
        self.roots
            .lock()
            .map(|roots| {
                roots
                    .iter()
                    .map(|(token, path)| LocalRoot {
                        token: token.clone(),
                        path: path.to_string_lossy().into_owned(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn local(&self, token: &str, path: &str, allow_missing: bool) -> Result<PathBuf, String> {
        let root = self
            .roots
            .lock()
            .map_err(|_| "폴더 잠금 오류")?
            .get(token)
            .cloned()
            .ok_or("로컬 폴더를 먼저 선택하세요")?;
        let parts = relative(path)?;
        let mut target = root.clone();
        for (index, part) in parts.iter().enumerate() {
            target.push(part);
            match std::fs::symlink_metadata(&target) {
                Ok(metadata) => {
                    if reparse(&metadata) {
                        return Err("심볼릭 링크/정션은 접근하지 않습니다".into());
                    }
                    let resolved =
                        std::fs::canonicalize(&target).map_err(|_| "로컬 경로 확인 실패")?;
                    if !resolved.starts_with(&root) {
                        return Err("선택한 로컬 폴더 밖의 경로입니다".into());
                    }
                }
                Err(e)
                    if allow_missing
                        && e.kind() == std::io::ErrorKind::NotFound
                        && index + 1 == parts.len() => {}
                Err(_) => return Err(message("로컬 경로 접근 실패")),
            }
        }
        Ok(target)
    }
    pub fn local_list(&self, token: &str, path: &str) -> Result<Listing, String> {
        let directory = self.local(token, path, false)?;
        let mut entries = vec![];
        for item in std::fs::read_dir(directory).map_err(|_| message("로컬 목록 실패"))? {
            if entries.len() >= 10000 {
                return Err("폴더 항목은 최대 10,000개까지 표시합니다".into());
            }
            let item = item.map_err(|_| message("로컬 항목 읽기 실패"))?;
            let name = item.file_name().to_string_lossy().into_owned();
            let meta = std::fs::symlink_metadata(item.path())
                .map_err(|_| message("로컬 항목 정보 실패"))?;
            entries.push(Entry {
                name: name.clone(),
                directory: meta.is_dir() && !reparse(&meta),
                blocked: reparse(&meta)
                    || safe_name(&name).is_err()
                    || (!meta.is_file() && !meta.is_dir()),
                size: meta.len(),
                modified: seconds(meta.modified()),
                mode: None,
            });
        }
        sort_entries(&mut entries);
        Ok(Listing {
            path: path.into(),
            entries,
        })
    }
    pub async fn session(&self, runtime: &Runtime, id: &str) -> Result<Arc<SftpSession>, String> {
        let ssh = runtime.handle(id)?;
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(id) {
            return Ok(session.clone());
        }
        let mut channel = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            ssh.channel_open_session(),
        )
        .await
        .map_err(|_| "SFTP 채널 시간 초과")?
        .map_err(|_| message("SFTP 채널 열기 실패"))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|_| message("SFTP 요청 실패"))?;
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                match channel.wait().await {
                    Some(russh::ChannelMsg::Success) => break Ok(()),
                    Some(russh::ChannelMsg::Failure) | None => {
                        break Err("서버가 SFTP를 지원하지 않거나 접근을 거부했습니다".to_string());
                    }
                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| "SFTP 응답 시간 초과")??;
        let session = Arc::new(
            SftpSession::new(channel.into_stream())
                .await
                .map_err(|_| message("SFTP 초기화 실패"))?,
        );
        session.set_timeout(15);
        sessions.insert(id.into(), session.clone());
        Ok(session)
    }
    pub async fn close(&self, id: &str) {
        self.cancel_connection(id);
        if let Some(session) = self.sessions.lock().await.remove(id) {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(3), session.close()).await;
        }
    }
    pub async fn remote_list(
        &self,
        runtime: &Runtime,
        id: &str,
        path: &str,
    ) -> Result<Listing, String> {
        remote_path(path)?;
        let s = self.session(runtime, id).await?;
        let path = s
            .canonicalize(if path.is_empty() { "." } else { path })
            .await
            .map_err(|_| message("원격 경로 열기 실패"))?;
        remote_path(&path)?;
        let mut entries = vec![];
        for item in s
            .read_dir(&path)
            .await
            .map_err(|_| message("원격 목록 실패"))?
        {
            if entries.len() >= 10000 {
                return Err("폴더 항목은 최대 10,000개까지 표시합니다".into());
            }
            let name = item.file_name();
            let meta = item.metadata();
            entries.push(Entry {
                name: name.clone(),
                directory: meta.is_dir() && !meta.is_symlink(),
                blocked: meta.is_symlink()
                    || safe_name(&name).is_err()
                    || (!meta.is_dir() && !meta.is_regular()),
                size: meta.len(),
                modified: meta.mtime.unwrap_or(0) as u64,
                mode: meta.permissions,
            });
        }
        sort_entries(&mut entries);
        Ok(Listing { path, entries })
    }
    pub fn jobs(&self) -> Vec<Transfer> {
        let mut jobs = self
            .jobs
            .lock()
            .map(|j| j.values().map(|x| x.view.clone()).collect::<Vec<_>>())
            .unwrap_or_default();
        jobs.sort_by(|a, b| a.id.cmp(&b.id));
        jobs
    }
    pub fn active(&self, id: &str) -> usize {
        self.jobs()
            .iter()
            .filter(|j| {
                j.connection_id == id
                    && matches!(
                        j.status.as_str(),
                        "queued" | "scanning" | "running" | "waiting"
                    )
            })
            .count()
    }
    pub fn cancel_connection(&self, id: &str) {
        if let Ok(jobs) = self.jobs.lock() {
            for job in jobs.values().filter(|j| j.spec.connection_id == id) {
                job.cancel.cancel();
            }
        }
    }
    pub fn cancel(&self, id: &str) -> Result<(), String> {
        self.jobs
            .lock()
            .map_err(|_| "전송 잠금 오류")?
            .get(id)
            .ok_or("전송을 찾을 수 없습니다")?
            .cancel
            .cancel();
        Ok(())
    }
    fn edit(&self, id: &str, f: impl FnOnce(&mut Transfer)) {
        if let Ok(mut jobs) = self.jobs.lock()
            && let Some(job) = jobs.get_mut(id)
        {
            f(&mut job.view);
        }
    }
    pub fn decide(&self, id: &str, decision: &str) -> Result<(), String> {
        if !["skip", "overwrite", "rename", "cancel"].contains(&decision) {
            return Err("충돌 처리 선택 오류".into());
        }
        let tx = self
            .jobs
            .lock()
            .map_err(|_| "전송 잠금 오류")?
            .get_mut(id)
            .and_then(|j| j.decision.take())
            .ok_or("충돌 확인 요청이 없습니다")?;
        tx.send(decision.into())
            .map_err(|_| "전송이 종료되었습니다".into())
    }
    pub fn retry(self: &Arc<Self>, runtime: Arc<Runtime>, id: &str) -> Result<String, String> {
        let spec = {
            let jobs = self.jobs.lock().map_err(|_| "전송 잠금 오류")?;
            let job = jobs.get(id).ok_or("전송을 찾을 수 없습니다")?;
            if !matches!(job.view.status.as_str(), "failed" | "cancelled") {
                return Err("실패/취소한 전송만 재시도할 수 있습니다".into());
            }
            job.spec.clone()
        };
        self.enqueue(runtime, spec)
    }
    pub fn enqueue(
        self: &Arc<Self>,
        runtime: Arc<Runtime>,
        spec: TransferSpec,
    ) -> Result<String, String> {
        if !["upload", "download"].contains(&spec.direction.as_str())
            || !["ask", "skip", "overwrite", "rename"].contains(&spec.collision.as_str())
            || spec.names.is_empty()
            || spec.names.len() > 1000
        {
            return Err("전송 요청을 확인하세요".into());
        }
        for name in &spec.names {
            safe_name(name)?;
        }
        relative(&spec.local_dir)?;
        remote_path(&spec.remote_dir)?;
        self.local(&spec.local_root, &spec.local_dir, false)?;
        runtime.handle(&spec.connection_id)?;
        if runtime
            .sessions
            .lock()
            .map_err(|_| "연결 잠금 오류")?
            .get(&spec.connection_id)
            .is_some_and(|s| s.detached)
        {
            return Err("백그라운드 연결에는 전송을 추가할 수 없습니다".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = CancellationToken::new();
        {
            let mut jobs = self.jobs.lock().map_err(|_| "전송 잠금 오류")?;
            if jobs
                .values()
                .filter(|j| {
                    matches!(
                        j.view.status.as_str(),
                        "queued" | "scanning" | "running" | "waiting"
                    )
                })
                .count()
                >= 100
            {
                return Err("전송 대기열 한도 100개를 초과했습니다".into());
            }
            if jobs.len() >= 300 {
                let expired = jobs
                    .iter()
                    .find(|(_, j)| {
                        matches!(j.view.status.as_str(), "completed" | "failed" | "cancelled")
                    })
                    .map(|(id, _)| id.clone());
                if let Some(expired) = expired {
                    jobs.remove(&expired);
                }
            }
            jobs.insert(
                id.clone(),
                Job {
                    spec: spec.clone(),
                    cancel: cancel.clone(),
                    decision: None,
                    view: Transfer {
                        id: id.clone(),
                        connection_id: spec.connection_id.clone(),
                        direction: spec.direction.clone(),
                        name: format!(
                            "{}{}",
                            spec.names
                                .iter()
                                .take(3)
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(", "),
                            if spec.names.len() > 3 {
                                format!(" 외 {}개", spec.names.len() - 3)
                            } else {
                                String::new()
                            }
                        ),
                        status: "queued".into(),
                        bytes: 0,
                        total: 0,
                        files: 0,
                        skipped: 0,
                        speed: 0,
                        message: "대기 중".into(),
                        conflict: None,
                    },
                },
            );
        }
        let manager = self.clone();
        let job_id = id.clone();
        tauri::async_runtime::spawn(async move {
            let result = tokio::select! {_=cancel.cancelled()=>Err("전송 취소".into()),permit=manager.permits.acquire()=>{let _permit=permit.map_err(|_|"전송 매니저 종료");manager.execute(&runtime,&job_id,&spec,&cancel).await}};
            manager.edit(&job_id, |job| {
                job.status = if cancel.is_cancelled() {
                    "cancelled"
                } else if result.is_ok() {
                    "completed"
                } else {
                    "failed"
                }
                .into();
                job.conflict = None;
                job.message = result.err().unwrap_or_else(|| {
                    if job.message.starts_with("이전 파일") {
                        job.message.clone()
                    } else {
                        "전송 완료".into()
                    }
                });
            });
            if manager.active(&spec.connection_id) == 0
                && runtime
                    .sessions
                    .lock()
                    .is_ok_and(|s| s.get(&spec.connection_id).is_some_and(|s| s.detached))
            {
                runtime.close(&spec.connection_id);
            }
        });
        Ok(id)
    }
    async fn collision(
        &self,
        id: &str,
        policy: &str,
        path: &str,
        cancel: &CancellationToken,
    ) -> Result<String, String> {
        if policy != "ask" {
            return Ok(policy.into());
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut jobs = self.jobs.lock().map_err(|_| "전송 잠금 오류")?;
            let job = jobs.get_mut(id).ok_or("전송 종료")?;
            job.view.status = "waiting".into();
            job.view.conflict = Some(path.into());
            job.decision = Some(tx);
        }
        let choice = tokio::select! {_=cancel.cancelled()=>Err("전송 취소".into()),result=rx=>result.map_err(|_|"충돌 확인 취소".to_string())}?;
        self.edit(id, |job| {
            job.status = "running".into();
            job.conflict = None;
        });
        if choice == "cancel" {
            cancel.cancel();
            return Err("전송 취소".into());
        }
        Ok(choice)
    }
    async fn execute(
        &self,
        runtime: &Runtime,
        id: &str,
        spec: &TransferSpec,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        let s = self.session(runtime, &spec.connection_id).await?;
        self.edit(id, |j| {
            j.status = "scanning".into();
            j.message = "폴더/파일 확인 중".into();
        });
        let remote = s
            .canonicalize(&spec.remote_dir)
            .await
            .map_err(|_| message("원격 폴더 확인 실패"))?;
        remote_path(&remote)?;
        let mut pending = spec
            .names
            .iter()
            .map(|n| (child(&spec.local_dir, n), child(&remote, n), 0usize))
            .collect::<Vec<_>>();
        let mut plan = vec![];
        let mut total = 0u64;
        while let Some((local, remote, depth)) = pending.pop() {
            if cancel.is_cancelled() {
                return Err("전송 취소".into());
            }
            if depth > 64 || plan.len() + pending.len() > 10000 {
                return Err("폴더 전송은 깊이 64/항목 10,000개로 제한됩니다".into());
            }
            let (directory, size) = if spec.direction == "upload" {
                let path = self.local(&spec.local_root, &local, false)?;
                let meta = tokio::fs::symlink_metadata(path)
                    .await
                    .map_err(|_| message("업로드 원본 확인 실패"))?;
                if reparse(&meta) || (!meta.is_dir() && !meta.is_file()) {
                    return Err("링크/특수 파일은 전송하지 않습니다".into());
                }
                (meta.is_dir(), meta.len())
            } else {
                let meta = s
                    .symlink_metadata(&remote)
                    .await
                    .map_err(|_| message("다운로드 원본 확인 실패"))?;
                if meta.is_symlink() || (!meta.is_dir() && !meta.is_regular()) {
                    return Err("링크/특수 파일은 전송하지 않습니다".into());
                }
                (meta.is_dir(), meta.len())
            };
            if directory {
                let names = if spec.direction == "upload" {
                    self.local_list(&spec.local_root, &local)?
                        .entries
                        .into_iter()
                        .map(|e| e.name)
                        .collect::<Vec<_>>()
                } else {
                    s.read_dir(&remote)
                        .await
                        .map_err(|_| message("원격 하위 폴더 읽기 실패"))?
                        .map(|e| e.file_name())
                        .collect()
                };
                for name in names {
                    safe_name(&name)?;
                    pending.push((child(&local, &name), child(&remote, &name), depth + 1));
                }
            } else {
                total = total.checked_add(size).ok_or("전송 크기 초과")?;
            }
            plan.push((local, remote, directory, size));
        }
        self.edit(id, |j| {
            j.status = "running".into();
            j.total = total;
            j.message = "전송 중".into();
        });
        let started = Instant::now();
        for (local, remote, directory, size) in plan {
            if cancel.is_cancelled() {
                return Err("전송 취소".into());
            }
            if directory {
                if spec.direction == "upload" {
                    match remote_meta(&s, &remote).await? {
                        Some(meta) if meta.is_dir() && !meta.is_symlink() => {}
                        Some(_) => return Err("원격 대상 폴더와 파일이 충돌합니다".into()),
                        None => s
                            .create_dir(&remote)
                            .await
                            .map_err(|_| message("원격 폴더 생성 실패"))?,
                    }
                } else {
                    let target = self.local(&spec.local_root, &local, true)?;
                    if target.exists() {
                        if !target.is_dir() {
                            return Err("로컬 대상 폴더와 파일이 충돌합니다".into());
                        }
                    } else {
                        tokio::fs::create_dir(&target)
                            .await
                            .map_err(|_| message("로컬 폴더 생성 실패"))?;
                    }
                }
                continue;
            }
            let done = if spec.direction == "upload" {
                self.upload(&s, id, spec, &local, &remote, size, cancel, started)
                    .await?
            } else {
                self.download(&s, id, spec, &local, &remote, size, cancel, started)
                    .await?
            };
            self.edit(id, |j| {
                if done {
                    j.files += 1;
                } else {
                    j.skipped += 1;
                }
            });
        }
        Ok(())
    }
    fn progress(&self, id: &str, n: u64, started: Instant) {
        self.edit(id, |j| {
            j.bytes += n;
            j.speed = (j.bytes as f64 / started.elapsed().as_secs_f64().max(0.001)) as u64;
        });
    }
    #[allow(clippy::too_many_arguments)]
    async fn upload(
        &self,
        s: &SftpSession,
        id: &str,
        spec: &TransferSpec,
        local: &str,
        remote: &str,
        size: u64,
        cancel: &CancellationToken,
        started: Instant,
    ) -> Result<bool, String> {
        let existing = remote_meta(s, remote).await?;
        if existing
            .as_ref()
            .is_some_and(|m| !m.is_regular() || m.is_symlink())
        {
            return Err("원격 대상이 일반 파일이 아닙니다".into());
        }
        let choice = if existing.is_some() {
            self.collision(id, &spec.collision, remote, cancel).await?
        } else {
            "new".into()
        };
        if choice == "skip" {
            return Ok(false);
        }
        let mut target = remote.to_string();
        if choice == "rename" {
            target = unique_remote(s, remote).await?;
        }
        let parent = target.rsplit_once('/').map_or("", |(p, _)| p);
        let temp = format!("{parent}/.nexaterm-{}.tmp", uuid::Uuid::new_v4());
        let path = self.local(&spec.local_root, local, false)?;
        let mut input = tokio::fs::File::open(&path)
            .await
            .map_err(|_| message("업로드 파일 열기 실패"))?;
        let mut output = s
            .open_with_flags(
                &temp,
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE,
            )
            .await
            .map_err(|_| message("임시 업로드 파일 생성 실패"))?;
        let result = async {
            let mut buffer = vec![0u8; 131072];
            let mut transferred = 0;
            loop {
                let count = tokio::select! {
                    _ = cancel.cancelled() => return Err("전송 취소".into()),
                    count = input.read(&mut buffer) => count.map_err(|_| message("로컬 파일 읽기 실패"))?
                };
                if count == 0 { break; }
                tokio::select! {
                    _ = cancel.cancelled() => return Err("전송 취소".into()),
                    result = output.write_all(&buffer[..count]) => result.map_err(|_| message("업로드 쓰기 실패"))?
                }
                transferred += count as u64;
                self.progress(id,count as u64,started);
            }
            output.shutdown().await.map_err(|_| message("업로드 완료 확인 실패"))?;
            if transferred != size { return Err("전송 중 원본 크기가 변경되었습니다".into()); }
            if cancel.is_cancelled() { return Err("전송 취소".into()); }
            if choice == "overwrite" {
                let current = remote_meta(s,&target).await?;
                if current.as_ref().map(|m|(m.size,m.mtime,m.permissions)) != existing.as_ref().map(|m|(m.size,m.mtime,m.permissions)) { return Err("전송 중 원격 대상이 변경되었습니다".into()); }
                let rollback = format!("{parent}/.nexaterm-{}.previous",uuid::Uuid::new_v4());
                s.rename(&target,&rollback).await.map_err(|_|message("기존 원격 파일 보관 실패"))?;
                if s.rename(&temp,&target).await.is_err() {
                    let recovered = s.rename(&rollback,&target).await.is_ok();
                    return Err(if recovered { "업로드 교체 실패: 기존 파일 복구됨".into() } else { format!("교체/복구 실패. 기존 파일 보관 위치: {rollback}") });
                }
                if s.remove_file(&rollback).await.is_err() { self.edit(id,|j|j.message=format!("이전 파일 보관본: {rollback}")); }
            } else { s.rename(&temp,&target).await.map_err(|_|message("업로드 파일 확정 실패 (대상 충돌 가능)"))?; }
            Ok(true)
        }.await;
        let _ = output.shutdown().await;
        if result.is_err() {
            let _ = s.remove_file(&temp).await;
        }
        result
    }
    #[allow(clippy::too_many_arguments)]
    async fn download(
        &self,
        s: &SftpSession,
        id: &str,
        spec: &TransferSpec,
        local: &str,
        remote: &str,
        size: u64,
        cancel: &CancellationToken,
        started: Instant,
    ) -> Result<bool, String> {
        let mut target = self.local(&spec.local_root, local, true)?;
        let existing = tokio::fs::symlink_metadata(&target).await.ok();
        if existing
            .as_ref()
            .is_some_and(|m| !m.is_file() || reparse(m))
        {
            return Err("로컬 대상이 일반 파일이 아닙니다".into());
        }
        let choice = if existing.is_some() {
            self.collision(id, &spec.collision, local, cancel).await?
        } else {
            "new".into()
        };
        if choice == "skip" {
            return Ok(false);
        }
        if choice == "rename" {
            target = unique_local(&target)?;
        }
        let parent = target.parent().ok_or("로컬 대상 폴더 오류")?;
        let temp = tempfile::NamedTempFile::new_in(parent)
            .map_err(|_| message("임시 다운로드 파일 생성 실패"))?;
        let mut output =
            tokio::fs::File::from_std(temp.as_file().try_clone().map_err(|_| "임시 파일 오류")?);
        let mut input = s
            .open(remote)
            .await
            .map_err(|_| message("원격 파일 열기 실패"))?;
        let mut buffer = vec![0u8; 131072];
        let mut transferred = 0;
        loop {
            let count = tokio::select! {_=cancel.cancelled()=>return Err("전송 취소".into()),count=input.read(&mut buffer)=>count.map_err(|_|message("다운로드 읽기 실패"))?};
            if count == 0 {
                break;
            }
            output
                .write_all(&buffer[..count])
                .await
                .map_err(|_| message("로컬 파일 쓰기 실패"))?;
            transferred += count as u64;
            self.progress(id, count as u64, started);
        }
        output
            .sync_all()
            .await
            .map_err(|_| message("다운로드 저장 실패"))?;
        drop(output);
        if transferred != size {
            return Err("전송 중 원본 크기가 변경되었습니다".into());
        }
        if cancel.is_cancelled() {
            return Err("전송 취소".into());
        }
        self.local(&spec.local_root, local, true)?;
        if choice == "overwrite" {
            let current = tokio::fs::symlink_metadata(&target).await.ok();
            if current.as_ref().map(|m| (m.len(), seconds(m.modified())))
                != existing.as_ref().map(|m| (m.len(), seconds(m.modified())))
            {
                return Err("전송 중 로컬 대상이 변경되었습니다".into());
            }
            temp.persist(&target)
                .map_err(|_| message("다운로드 교체 실패"))?;
        } else {
            temp.persist_noclobber(&target)
                .map_err(|_| message("다운로드 확정 실패 (대상 충돌 가능)"))?;
        }
        Ok(true)
    }
}
fn sort_entries(entries: &mut [Entry]) {
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}
async fn remote_meta(s: &SftpSession, path: &str) -> Result<Option<FileAttributes>, String> {
    match s.symlink_metadata(path).await {
        Ok(m) => Ok(Some(m)),
        Err(russh_sftp::client::error::Error::Status(status))
            if status.status_code == russh_sftp::protocol::StatusCode::NoSuchFile =>
        {
            Ok(None)
        }
        Err(_) => Err(message("원격 대상 확인 실패")),
    }
}
async fn unique_remote(s: &SftpSession, path: &str) -> Result<String, String> {
    for index in 1..=1000 {
        let candidate = format!("{path} ({index})");
        if remote_meta(s, &candidate).await?.is_none() {
            return Ok(candidate);
        }
    }
    Err("이름 변경 후보가 너무 많습니다".into())
}
fn unique_local(path: &Path) -> Result<PathBuf, String> {
    let name = path.file_name().ok_or("대상 이름 오류")?.to_string_lossy();
    for index in 1..=1000 {
        let candidate = path.with_file_name(format!("{name} ({index})"));
        if !candidate.try_exists().map_err(|_| "대상 확인 실패")? {
            return Ok(candidate);
        }
    }
    Err("이름 변경 후보가 너무 많습니다".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opaque_binary_handles_roundtrip_without_utf8_loss() {
        for raw in [
            (0..=255).collect::<Vec<u8>>(),
            "\u{e000}nxt-handle:literal".as_bytes().to_vec(),
            b"ordinary-handle".to_vec(),
        ] {
            let mut wire = 1u32.to_be_bytes().to_vec();
            wire.extend_from_slice(&(raw.len() as u32).to_be_bytes());
            wire.extend_from_slice(&raw);
            let mut input = wire.clone().into();
            let handle: russh_sftp::protocol::Handle =
                russh_sftp::de::from_bytes(&mut input).unwrap();
            assert_eq!(
                russh_sftp::ser::to_bytes(&handle).unwrap().as_ref(),
                wire.as_slice()
            );
            let close = russh_sftp::protocol::Close {
                id: 1,
                handle: handle.handle,
            };
            assert_eq!(
                russh_sftp::ser::to_bytes(&close).unwrap().as_ref(),
                wire.as_slice()
            );
        }
    }
    #[test]
    fn rejects_path_escape_and_windows_aliases() {
        for name in [
            "..", "a/b", "a\\b", "x:stream", "CON.txt", "LPT1", "trail.", "trail ",
        ] {
            assert!(safe_name(name).is_err());
        }
        assert!(safe_name("한글 파일.txt").is_ok());
        let temp = tempfile::tempdir().unwrap();
        let manager = FileManager::default();
        let root = manager.grant_root(temp.path()).unwrap();
        assert!(manager.local(&root.token, "../outside", true).is_err());
        assert!(manager.local(&root.token, "C:/escape", true).is_err());
        assert!(manager.local("ungranted", "", false).is_err());
    }
}
