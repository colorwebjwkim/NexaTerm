use crate::{
    models::Profile,
    ssh::{self, Control, Event, Runtime},
    store::Store,
};
use russh::{
    Channel, ChannelId, Pty,
    keys::*,
    server::{self, Server as _, Session},
};
use std::{sync::Arc, time::Duration};
use tauri::ipc::{Channel as IpcChannel, InvokeResponseBody};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

#[derive(Clone)]
struct Fixture;
impl server::Server for Fixture {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        Self
    }
}
impl server::Handler for Fixture {
    type Error = russh::Error;
    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<server::Auth, Self::Error> {
        Ok(if user == "fixture" && password == "fixture-ssh-password" {
            server::Auth::Accept
        } else {
            server::Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            }
        })
    }
    async fn auth_publickey(
        &mut self,
        user: &str,
        _: &PublicKey,
    ) -> Result<server::Auth, Self::Error> {
        Ok(if user == "fixture" {
            server::Auth::Accept
        } else {
            server::Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            }
        })
    }
    async fn channel_open_session(
        &mut self,
        _: Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _: &str,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }
    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        session.data(
            channel,
            b"\x1b[32mNexaTerm SSH fixture\x1b[0m\r\n".as_slice(),
        )?;
        Ok(())
    }
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if data == b"fixture-output-flood" {
            session.data(channel, vec![b'X'; 1024 * 1024])?;
            return Ok(());
        }
        session.data(channel, data.to_vec())?;
        Ok(())
    }
    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        cols: u32,
        rows: u32,
        _: u32,
        _: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.data(channel, format!("SIZE={cols}x{rows}\r\n").into_bytes())?;
        Ok(())
    }
}
async fn server(port: Option<u16>) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port.unwrap_or(0)))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = Arc::new(server::Config {
        keys: vec![PrivateKey::random(&mut rand_new::rng(), Algorithm::Ed25519).unwrap()],
        auth_rejection_time: Duration::from_millis(1),
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let task = tokio::spawn(async move {
        let mut fixture = Fixture;
        let _ = fixture.run_on_socket(config, &listener).await;
    });
    (port, task)
}
fn profile(port: u16) -> Profile {
    Profile {
        connection_type: "ssh".into(),
        host_key_algorithm: "ssh-rsa".into(),
        id: uuid::Uuid::new_v4().to_string(),
        name: "fixture".into(),
        host: "127.0.0.1".into(),
        port,
        username: "fixture".into(),
        group: String::new(),
        tags: vec![],
        favorite: false,
        auth_method: "password".into(),
        key_path: String::new(),
        secret_ref: None,
    }
}
fn channel() -> (
    IpcChannel<Event>,
    mpsc::UnboundedReceiver<serde_json::Value>,
) {
    let (tx, rx) = mpsc::unbounded_channel();
    (
        IpcChannel::new(move |body| {
            if let InvokeResponseBody::Json(json) = body {
                let _ = tx.send(serde_json::from_str(&json).unwrap());
            }
            Ok(())
        }),
        rx,
    )
}
async fn next(rx: &mut mpsc::UnboundedReceiver<serde_json::Value>) -> serde_json::Value {
    tokio::time::timeout(Duration::from_secs(15), rx.recv())
        .await
        .expect("SSH fixture event timeout")
        .expect("SSH fixture channel closed")
}
async fn approve(runtime: &Runtime, event: &serde_json::Value, allow: bool) {
    let token = event["token"].as_str().unwrap();
    runtime
        .approvals
        .lock()
        .unwrap()
        .remove(token)
        .unwrap()
        .sender
        .send(allow)
        .unwrap();
}
async fn wait_connected(
    runtime: &Runtime,
    rx: &mut mpsc::UnboundedReceiver<serde_json::Value>,
    allow: bool,
) -> bool {
    loop {
        let event = next(rx).await;
        match event["kind"].as_str() {
            Some("hostKey") => approve(runtime, &event, allow).await,
            Some("state") if event["state"] == "connected" => return true,
            Some("state") if event["state"] == "error" => return false,
            _ => {}
        }
    }
}
#[tokio::test]
async fn ssh_password_independent_connections_io_resize_and_host_key_change() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&directory.path().join("state.db")).unwrap());
    let runtime = Arc::new(Runtime::default());
    let (port, server) = server(None).await;
    let p = profile(port);
    let (events, mut rx) = channel();
    let a = ssh::start(
        store.clone(),
        runtime.clone(),
        p.clone(),
        Zeroizing::new("fixture-ssh-password".into()),
        events,
    )
    .unwrap();
    assert!(wait_connected(&runtime, &mut rx, true).await);
    let (events, mut rx_b) = channel();
    let b = ssh::start(
        store.clone(),
        runtime.clone(),
        p.clone(),
        Zeroizing::new("fixture-ssh-password".into()),
        events,
    )
    .unwrap();
    assert_ne!(a, b);
    assert!(wait_connected(&runtime, &mut rx_b, false).await);
    runtime
        .send(&a, Control::Input("독립 탭 A".as_bytes().to_vec()))
        .unwrap();
    runtime.send(&a, Control::Resize(123, 45)).unwrap();
    let mut output = String::new();
    while !output.contains("독립 탭 A") || !output.contains("SIZE=123x45") {
        let event = next(&mut rx).await;
        if event["kind"] == "data" {
            let bytes: Vec<u8> = serde_json::from_value(event["data"].clone()).unwrap();
            output.push_str(&String::from_utf8_lossy(&bytes));
            runtime
                .send(&a, Control::Ack(event["sequence"].as_u64().unwrap()))
                .unwrap();
        }
    }
    runtime.close(&a);
    assert!(runtime.sessions.lock().unwrap().contains_key(&b));
    runtime.close(&b);
    let (events, mut bad_rx) = channel();
    ssh::start(
        store.clone(),
        runtime.clone(),
        p.clone(),
        Zeroizing::new("wrong-password".into()),
        events,
    )
    .unwrap();
    assert!(!wait_connected(&runtime, &mut bad_rx, true).await);
    let before = store.read().unwrap().trusted_keys.clone();
    server.abort();
    let _ = server.await;
    let (_, replacement) = self::server(Some(port)).await;
    let (events, mut changed_rx) = channel();
    ssh::start(
        store.clone(),
        runtime.clone(),
        p,
        Zeroizing::new("fixture-ssh-password".into()),
        events,
    )
    .unwrap();
    let event = next(&mut changed_rx).await;
    let event = if event["kind"] == "hostKey" {
        event
    } else {
        next(&mut changed_rx).await
    };
    assert_eq!(event["kind"], "hostKey");
    assert!(event["previous"].is_string());
    approve(&runtime, &event, false).await;
    assert!(!wait_connected(&runtime, &mut changed_rx, false).await);
    assert_eq!(before, store.read().unwrap().trusted_keys);
    runtime.close_all();
    replacement.abort();
}
#[tokio::test]
async fn ssh_private_key_authentication() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fixture_key");
    let key = PrivateKey::random(&mut rand_new::rng(), Algorithm::Ed25519).unwrap();
    let key = key
        .encrypt(&mut rand_new::rng(), "fixture-passphrase")
        .unwrap();
    key.write_openssh_file(&path, Default::default()).unwrap();
    let store = Arc::new(Store::open(&directory.path().join("state.db")).unwrap());
    let runtime = Arc::new(Runtime::default());
    let (port, server) = server(None).await;
    let mut p = profile(port);
    p.auth_method = "privateKey".into();
    p.key_path = path.to_string_lossy().into_owned();
    let (events, mut rx) = channel();
    let id = ssh::start(
        store,
        runtime.clone(),
        p,
        Zeroizing::new("fixture-passphrase".into()),
        events,
    )
    .unwrap();
    assert!(wait_connected(&runtime, &mut rx, true).await);
    runtime.close(&id);
    server.abort();
}

#[tokio::test]
async fn explicit_host_key_selection_mismatch_and_retry() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&directory.path().join("state.db")).unwrap());
    let runtime = Arc::new(Runtime::default());
    let (port, server) = server(None).await;
    let mut p = profile(port);
    p.host_key_algorithm = "rsa-sha2-512".into();
    let (events, mut rx) = channel();
    ssh::start(
        store.clone(),
        runtime.clone(),
        p.clone(),
        Zeroizing::new("fixture-ssh-password".into()),
        events,
    )
    .unwrap();
    assert!(!wait_connected(&runtime, &mut rx, true).await);
    p.host_key_algorithm = "ssh-ed25519".into();
    let (events, mut rx) = channel();
    ssh::start(
        store,
        runtime.clone(),
        p,
        Zeroizing::new("fixture-ssh-password".into()),
        events,
    )
    .unwrap();
    assert!(wait_connected(&runtime, &mut rx, true).await);
    runtime.close_all();
    server.abort();
}

#[tokio::test]
async fn terminal_output_continues_after_missing_ack() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&directory.path().join("state.db")).unwrap());
    let runtime = Arc::new(Runtime::default());
    let (port, server) = server(None).await;
    let (events, mut rx) = channel();
    let id = ssh::start(
        store,
        runtime.clone(),
        profile(port),
        Zeroizing::new("fixture-ssh-password".into()),
        events,
    )
    .unwrap();
    assert!(wait_connected(&runtime, &mut rx, true).await);
    runtime
        .send(&id, Control::Input(b"fixture-output-flood".to_vec()))
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let mut received = 0;
        let mut skipped = false;
        while received < 1024 * 1024 {
            let event = rx.recv().await.unwrap();
            if event["kind"] == "data" {
                let bytes: Vec<u8> = serde_json::from_value(event["data"].clone()).unwrap();
                received += bytes.iter().filter(|&&byte| byte == b'X').count();
                // Simulate one lost renderer ACK; every later callback confirms
                // all preceding writes have also finished parsing.
                if skipped {
                    runtime
                        .send(&id, Control::Ack(event["sequence"].as_u64().unwrap()))
                        .unwrap();
                } else {
                    skipped = true;
                }
            }
        }
    })
    .await;
    runtime.close_all();
    server.abort();
    assert!(result.is_ok(), "terminal froze after one lost ACK");
}

#[tokio::test]
async fn terminal_ack_survives_full_input_queue_and_reordering() {
    let runtime = Runtime::default();
    let (sender, mut receiver) = mpsc::channel(64);
    let (acknowledged, acknowledgements) = tokio::sync::watch::channel(0u64);
    let (stop, _cancelled) = tokio::sync::watch::channel(false);
    runtime.sessions.lock().unwrap().insert(
        "fixture".into(),
        ssh::SessionControl {
            sender,
            acknowledged,
            stop,
            ssh: None,
            detached: false,
        },
    );
    for _ in 0..64 {
        runtime.send("fixture", Control::Resize(100, 30)).unwrap();
    }
    assert!(runtime.send("fixture", Control::Resize(100, 30)).is_err());
    // ACK delivery is independent of the saturated 64-slot queue.
    runtime.send("fixture", Control::Ack(3)).unwrap();
    runtime.send("fixture", Control::Ack(1)).unwrap();
    runtime.send("fixture", Control::Ack(2)).unwrap();
    assert_eq!(*acknowledgements.borrow(), 3);
    for _ in 0..64 {
        assert!(matches!(receiver.try_recv(), Ok(Control::Resize(_, _))));
    }
    assert!(receiver.try_recv().is_err());
}
