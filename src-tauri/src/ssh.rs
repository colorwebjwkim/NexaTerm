use crate::{models::Profile, store::Store};
use russh::{
    ChannelMsg, Disconnect, client,
    keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate, load_secret_key},
};
use serde::Serialize;
use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::ipc::Channel;
use tokio::sync::{mpsc, oneshot, watch};
use zeroize::Zeroizing;

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Event {
    AuthenticationRejected,
    State {
        state: String,
        message: String,
    },
    Data {
        data: Vec<u8>,
        sequence: u64,
    },
    HostKey {
        token: String,
        host: String,
        algorithm: String,
        fingerprint: String,
        previous: Option<String>,
    },
}
pub enum Control {
    Input(Vec<u8>),
    Resize(u32, u32),
    Ack(u64),
    Detach,
}
pub struct SessionControl {
    pub sender: mpsc::Sender<Control>,
    pub acknowledged: watch::Sender<u64>,
    pub stop: watch::Sender<bool>,
    pub ssh: Option<Arc<client::Handle<Client>>>,
    pub detached: bool,
}
pub struct Approval {
    pub connection_id: String,
    pub changed: bool,
    pub sender: oneshot::Sender<bool>,
}
#[derive(Default)]
pub struct Runtime {
    pub sessions: Mutex<HashMap<String, SessionControl>>,
    pub approvals: Mutex<HashMap<String, Approval>>,
    pub files: Arc<crate::sftp::FileManager>,
}
impl Runtime {
    pub fn close_all(&self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            for (_, session) in sessions.drain() {
                let _ = session.stop.send(true);
            }
        }
        if let Ok(mut approvals) = self.approvals.lock() {
            approvals.clear();
        }
    }
    pub fn close(&self, id: &str) {
        self.files.cancel_connection(id);
        if let Ok(mut sessions) = self.sessions.lock()
            && let Some(session) = sessions.remove(id)
        {
            let _ = session.stop.send(true);
        }
    }
    pub fn handle(&self, id: &str) -> Result<Arc<client::Handle<Client>>, String> {
        self.sessions
            .lock()
            .map_err(|_| "연결 잠금 오류")?
            .get(id)
            .and_then(|s| s.ssh.clone())
            .ok_or("SSH 연결이 준비되지 않았습니다".into())
    }
    pub fn detach(&self, id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|_| "연결 잠금 오류")?;
        let session = sessions.get_mut(id).ok_or("연결이 종료되었습니다")?;
        session
            .sender
            .try_send(Control::Detach)
            .map_err(|_| "연결 입력이 처리 중입니다. 다시 시도하세요")?;
        session.detached = true;
        Ok(())
    }
    pub fn send(&self, id: &str, control: Control) -> Result<(), String> {
        let sessions = self.sessions.lock().map_err(|_| "연결 잠금 오류")?;
        let session = sessions.get(id).ok_or("연결이 종료되었습니다")?;
        if let Control::Ack(sequence) = control {
            // Rendering ACKs must not be dropped when input/resize queues fill.
            // xterm callbacks confirm all writes up to this sequence.
            session.acknowledged.send_if_modified(|current| {
                if sequence > *current {
                    *current = sequence;
                    true
                } else {
                    false
                }
            });
            return Ok(());
        }
        session
            .sender
            .try_send(control)
            .map_err(|_| "입력 대기열이 가득 찼거나 연결이 종료되었습니다".into())
    }
}
pub struct Client {
    store: Arc<Store>,
    runtime: Arc<Runtime>,
    events: Channel<Event>,
    endpoint: String,
    connection_id: String,
}
impl client::Handler for Client {
    type Error = russh::Error;
    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // Certificate trust requires a separate CA/principal policy; fail closed for now.
        if key.certificate().is_some() {
            return Ok(false);
        }
        let public = key.public_key();
        let fingerprint = public.fingerprint(HashAlg::Sha256).to_string();
        let algorithm = public.algorithm().to_string();
        let previous = match self.store.read() {
            Ok(doc) => doc.trusted_keys.get(&self.endpoint).cloned(),
            Err(_) => return Ok(false),
        };
        if previous.as_ref() == Some(&fingerprint) {
            return Ok(true);
        }
        let token = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        if let Ok(mut approvals) = self.runtime.approvals.lock() {
            approvals.insert(
                token.clone(),
                Approval {
                    connection_id: self.connection_id.clone(),
                    changed: previous.is_some(),
                    sender: tx,
                },
            );
        } else {
            return Ok(false);
        }
        let sent = self
            .events
            .send(Event::HostKey {
                token: token.clone(),
                host: self.endpoint.clone(),
                algorithm,
                fingerprint: fingerprint.clone(),
                previous: previous.clone(),
            })
            .is_ok();
        let approved = sent
            && matches!(
                tokio::time::timeout(Duration::from_secs(90), rx).await,
                Ok(Ok(true))
            );
        if let Ok(mut approvals) = self.runtime.approvals.lock() {
            approvals.remove(&token);
        }
        if !approved {
            return Ok(false);
        }
        // Compare-and-set prevents a concurrent approval from silently replacing another key.
        Ok(self
            .store
            .update(|doc| {
                let current = doc.trusted_keys.get(&self.endpoint).cloned();
                if current != previous && current.as_ref() != Some(&fingerprint) {
                    return Err("다른 연결에서 호스트키가 변경되었습니다".into());
                }
                doc.trusted_keys.insert(self.endpoint.clone(), fingerprint);
                Ok(())
            })
            .is_ok())
    }
}

pub fn start(
    store: Arc<Store>,
    runtime: Arc<Runtime>,
    profile: Profile,
    password: Zeroizing<String>,
    events: Channel<Event>,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let (sender, receiver) = mpsc::channel(64);
    let (acknowledged, acknowledgements) = watch::channel(0u64);
    let (stop, mut cancelled) = watch::channel(false);
    {
        let mut sessions = runtime.sessions.lock().map_err(|_| "연결 잠금 오류")?;
        if sessions.len() >= 32 {
            return Err("동시 연결은 최대 32개입니다".into());
        }
        sessions.insert(
            id.clone(),
            SessionControl {
                sender,
                acknowledged,
                stop,
                ssh: None,
                detached: false,
            },
        );
    }
    let connection_id = id.clone();
    tauri::async_runtime::spawn(async move {
        let _ = events.send(Event::State {
            state: "connecting".into(),
            message: if profile.connection_type == "sftp" {
                "SFTP 연결 중"
            } else {
                "SSH 연결 중"
            }
            .into(),
        });
        let result = tokio::select! {
            result=run(store,runtime.clone(),profile,password,events.clone(),connection_id.clone(),(receiver,acknowledgements)) => result,
            _=cancelled.changed() => Ok(())
        };
        runtime.close(&connection_id);
        runtime.files.close(&connection_id).await;
        if let Ok(mut approvals) = runtime.approvals.lock() {
            approvals.retain(|_, a| a.connection_id != connection_id);
        }
        let (state, message) = match result {
            Ok(()) => ("disconnected", "연결 종료".into()),
            Err(message) => ("error", message),
        };
        let _ = events.send(Event::State {
            state: state.into(),
            message,
        });
    });
    Ok(id)
}
async fn await_success(channel: &mut russh::Channel<client::Msg>) -> Result<(), String> {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Success) => return Ok(()),
                Some(ChannelMsg::Failure) | None => {
                    return Err("서버가 터미널 요청을 거부했습니다".into());
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| "터미널 요청 시간 초과")?
}
fn host_key_preferences(selection: &str) -> Result<russh::Preferred, String> {
    let mut preferred = russh::Preferred::default();
    if selection == "auto" {
        return Ok(preferred);
    }
    let algorithm = selection
        .parse::<russh::keys::Algorithm>()
        .map_err(|_| "호스트키 알고리즘이 올바르지 않습니다".to_string())?;
    if selection == "ssh-rsa" {
        let mut keys = preferred.key.to_vec();
        keys.retain(|key| key != &algorithm);
        keys.insert(0, algorithm);
        preferred.key = Cow::Owned(keys);
    } else {
        preferred.key = Cow::Owned(vec![algorithm]);
    }
    Ok(preferred)
}
async fn run(
    store: Arc<Store>,
    runtime: Arc<Runtime>,
    profile: Profile,
    password: Zeroizing<String>,
    events: Channel<Event>,
    id: String,
    (mut controls, mut acknowledgements): (mpsc::Receiver<Control>, watch::Receiver<u64>),
) -> Result<(), String> {
    let config = Arc::new(client::Config {
        preferred: host_key_preferences(&profile.host_key_algorithm)?,
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        window_size: 262144,
        channel_buffer_size: 8,
        nodelay: true,
        ..Default::default()
    });
    let handler = Client {
        store,
        runtime: runtime.clone(),
        events: events.clone(),
        endpoint: format!("[{}]:{}", profile.host.to_lowercase(), profile.port),
        connection_id: id.clone(),
    };
    let address = (profile.host.as_str(), profile.port);
    let socket = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::net::TcpStream::connect(address),
    )
    .await
    .map_err(|_| "접속 시간 초과")?
    .map_err(|_| "호스트에 연결할 수 없습니다")?;
    let mut session = tokio::time::timeout(
        Duration::from_secs(120),
        client::connect_stream(config, socket, handler),
    )
    .await
    .map_err(|_| "SSH 협상 또는 호스트키 확인 시간 초과")?
    .map_err(|error| format!("SSH 협상/호스트키 확인 실패: {error}. 프로필의 호스트키 알고리즘과 서버 설정을 확인하세요"))?;
    let _ = events.send(Event::State {
        state: "authenticating".into(),
        message: "인증 중".into(),
    });
    let authenticated = if profile.auth_method == "privateKey" {
        let pass = if password.is_empty() {
            None
        } else {
            Some(password.as_str())
        };
        let key = load_secret_key(&profile.key_path, pass)
            .map_err(|_| "개인키를 읽을 수 없거나 passphrase가 틀렸습니다")?;
        // RSA is enabled for public host-key verification. Keep private RSA
        // operations disabled while RUSTSEC-2023-0071 remains unresolved.
        if key.algorithm().is_rsa() {
            return Err("RSA 개인키 인증은 지원하지 않습니다. 비밀번호 또는 Ed25519/ECDSA 개인키를 사용하세요".into());
        }
        let hash = session
            .best_supported_rsa_hash()
            .await
            .map_err(|_| "키 알고리즘 협상 실패")?
            .flatten();
        tokio::time::timeout(
            Duration::from_secs(20),
            session.authenticate_publickey(
                &profile.username,
                PrivateKeyWithHashAlg::new(Arc::new(key), hash),
            ),
        )
        .await
        .map_err(|_| "인증 시간 초과")?
        .map_err(|_| "개인키 인증 실패")?
        .success()
    } else {
        tokio::time::timeout(
            Duration::from_secs(20),
            session.authenticate_password(&profile.username, password.as_str()),
        )
        .await
        .map_err(|_| "인증 시간 초과")?
        .map_err(|_| "비밀번호 인증 실패")?
        .success()
    };
    drop(password);
    if !authenticated {
        if profile.auth_method == "password" {
            let _ = events.send(Event::AuthenticationRejected);
        }
        return Err("인증 거부: 계정, 비밀번호 또는 키를 확인하세요".into());
    }
    let session = Arc::new(session);
    if let Some(control) = runtime
        .sessions
        .lock()
        .map_err(|_| "연결 잠금 오류")?
        .get_mut(&id)
    {
        control.ssh = Some(session.clone());
    }
    if profile.connection_type == "sftp" {
        runtime.files.session(&runtime, &id).await?;
        let _ = events.send(Event::State {
            state: "connected".into(),
            message: "SFTP 연결됨".into(),
        });
        let mut heartbeat = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                control = controls.recv() => if control.is_none() { break; },
                _ = heartbeat.tick() => if session.is_closed() { return Err("SFTP 연결이 종료되었습니다".into()); }
            }
        }
        let _ = session
            .disconnect(Disconnect::ByApplication, "", "en")
            .await;
        return Ok(());
    }
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|_| "터미널 채널 열기 실패")?;
    channel
        .request_pty(true, "xterm-256color", 100, 30, 0, 0, &[])
        .await
        .map_err(|_| "PTY 요청 실패")?;
    await_success(&mut channel).await?;
    channel
        .request_shell(true)
        .await
        .map_err(|_| "셸 요청 실패")?;
    await_success(&mut channel).await?;
    let _ = events.send(Event::State {
        state: "connected".into(),
        message: "SSH 연결됨".into(),
    });
    let mut sequence = 0u64;
    let mut outstanding = VecDeque::<(u64, usize)>::new();
    let mut pending_bytes = 0usize;
    let mut detached = false;
    let mut output_progress = tokio::time::Instant::now();
    loop {
        tokio::select! {
            control=controls.recv()=> match control {
                Some(Control::Input(bytes))=>channel.data_bytes(bytes).await.map_err(|_|"입력 전송 실패")?,
                Some(Control::Resize(cols,rows))=>channel.window_change(cols,rows,0,0).await.map_err(|_|"터미널 크기 변경 실패")?,
                Some(Control::Ack(_))=> {}, // Routed separately by Runtime::send.
                Some(Control::Detach)=> { channel.close().await.map_err(|_|"터미널 채널 종료 실패")?; detached=true; outstanding.clear(); pending_bytes=0; },
                None=>break
            },
            changed=acknowledgements.changed(), if !detached => {
                if changed.is_err() { break; }
                let ack = *acknowledgements.borrow_and_update();
                if ack <= sequence {
                    let before = pending_bytes;
                    while outstanding.front().is_some_and(|(n,_)| *n <= ack) {
                        pending_bytes -= outstanding.pop_front().unwrap().1;
                    }
                    if pending_bytes < before { output_progress = tokio::time::Instant::now(); }
                }
            },
            message=channel.wait(), if !detached && pending_bytes<131072 => match message {
                Some(ChannelMsg::Data{data})|Some(ChannelMsg::ExtendedData{data,..})=> {
                    if pending_bytes == 0 { output_progress = tokio::time::Instant::now(); }
                    sequence+=1; pending_bytes+=data.len(); outstanding.push_back((sequence,data.len()));
                    events.send(Event::Data{data:data.to_vec(),sequence}).map_err(|_|"터미널 화면이 종료되었습니다")?;
                },
                Some(ChannelMsg::Close)|None=>break,
                _=>{}
            },
            _=tokio::time::sleep_until(output_progress + Duration::from_secs(60)), if pending_bytes>=131072 => return Err(format!("터미널 출력 확인 시간 초과 (대기 {pending_bytes}바이트). 재접속하세요"))
        }
    }
    let _ = session
        .disconnect(Disconnect::ByApplication, "", "en")
        .await;
    Ok(())
}
