use crate::{
    backup,
    models::{Profile, Snapshot},
    store::Store,
};
use zeroize::Zeroizing;

#[test]
fn theme_defaults_persistence_backup_and_validation() {
    assert_eq!(crate::models::Settings::default().theme, "system");
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("theme.db");
    for theme in ["dark", "light", "system"] {
        let store = Store::open(&database).unwrap();
        let mut settings = store.read().unwrap().settings;
        settings.theme = theme.into();
        store.save_settings(settings).unwrap();
        drop(store);
        let store = Store::open(&database).unwrap();
        let snapshot = store.read().unwrap();
        assert_eq!(snapshot.settings.theme, theme);
        let encoded = backup::encode(&snapshot, "fixture-theme-backup", false).unwrap();
        let payload = backup::decode(&encoded, "fixture-theme-backup").unwrap();
        let decoded: backup::Backup = serde_json::from_slice(&payload).unwrap();
        backup::validate(&decoded).unwrap();
        assert_eq!(decoded.snapshot.settings.theme, theme);
        let restored_store = Store::open(&directory.path().join("restored.db")).unwrap();
        let restored = backup::restore(&restored_store, &payload, "replace").unwrap();
        assert_eq!(restored.settings.theme, theme);
        let mut invalid = decoded.snapshot.settings.clone();
        invalid.theme = "invalid".into();
        assert!(store.save_settings(invalid).is_err());
        assert_eq!(store.read().unwrap().settings.theme, theme);
    }
}

#[test]
fn exit_confirmation_defaults_and_survives_restart() {
    let legacy: crate::models::Settings =
        serde_json::from_str(r#"{"theme":"dark","fontSize":14,"scrollback":5000}"#).unwrap();
    assert!(legacy.confirm_exit);
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("settings.db");
    let store = Store::open(&database).unwrap();
    let mut settings = legacy;
    settings.font_size = 18;
    store.save_settings(settings).unwrap();
    store.set_confirm_exit(false).unwrap();
    drop(store);
    let store = Store::open(&database).unwrap();
    let saved = store.read().unwrap().settings;
    assert!(!saved.confirm_exit);
    assert_eq!(saved.font_size, 18);
    assert_eq!(saved.scrollback, 5000);
    let mut settings = saved;
    settings.confirm_exit = true;
    store.save_settings(settings).unwrap();
    drop(store);
    assert!(
        Store::open(&database)
            .unwrap()
            .read()
            .unwrap()
            .settings
            .confirm_exit
    );
}

fn profile() -> Profile {
    Profile {
        connection_type: "ssh".into(),
        host_key_algorithm: "ssh-rsa".into(),
        id: uuid::Uuid::new_v4().to_string(),
        name: "검증 서버".into(),
        host: "127.0.0.1".into(),
        port: 22,
        username: "fixture".into(),
        group: "개발".into(),
        tags: vec!["test".into()],
        favorite: true,
        auth_method: "password".into(),
        key_path: String::new(),
        secret_ref: None,
    }
}

#[test]
fn local_browser_default_restart_fallback_and_grant_reuse() {
    use crate::{file_commands::restore_local_location, sftp::FileManager};
    let directory = tempfile::tempdir().unwrap();
    let documents = directory.path().join("Documents");
    let last = documents.join("last-folder");
    std::fs::create_dir_all(&last).unwrap();
    std::fs::write(last.join("remembered.txt"), "fixture").unwrap();
    let database = directory.path().join("settings.db");
    let defaults = [documents.clone()];
    let store = Store::open(&database).unwrap();
    let files = FileManager::default();
    let initial = restore_local_location(&files, &store, &defaults).unwrap();
    assert_eq!(
        std::path::PathBuf::from(&initial.root.path),
        documents.canonicalize().unwrap()
    );
    assert_eq!(initial.listing.path, "");
    for _ in 0..80 {
        restore_local_location(&files, &store, &defaults).unwrap();
    }
    assert_eq!(files.roots().len(), 1);
    store
        .save_local_location(&initial.root.path, "last-folder")
        .unwrap();
    drop(store);
    let store = Store::open(&database).unwrap();
    let restored = restore_local_location(&FileManager::default(), &store, &defaults).unwrap();
    assert_eq!(restored.listing.path, "last-folder");
    assert_eq!(restored.listing.entries[0].name, "remembered.txt");
    assert!(
        !serde_json::to_string(&store.read().unwrap())
            .unwrap()
            .contains("last-folder")
    );
    // Neither a missing directory nor a tampered relative path can escape the grant.
    store
        .save_local_location(&initial.root.path, "missing-folder")
        .unwrap();
    assert_eq!(
        restore_local_location(&files, &store, &defaults)
            .unwrap()
            .listing
            .path,
        ""
    );
    store
        .save_local_location(&initial.root.path, "../")
        .unwrap();
    assert_eq!(
        restore_local_location(&files, &store, &defaults)
            .unwrap()
            .listing
            .path,
        ""
    );
    store
        .save_local_location(&directory.path().join("missing-root").to_string_lossy(), "")
        .unwrap();
    assert_eq!(
        restore_local_location(&files, &store, &defaults)
            .unwrap()
            .root
            .path,
        initial.root.path
    );
    assert!(restore_local_location(&FileManager::default(), &store, &[]).is_ok());
    let empty = Store::open(&directory.path().join("empty.db")).unwrap();
    assert!(restore_local_location(&FileManager::default(), &empty, &[]).is_err());
}

#[test]
fn connection_type_legacy_and_backup_compatibility() {
    let mut json = serde_json::to_value(profile()).unwrap();
    json.as_object_mut().unwrap().remove("connectionType");
    let legacy: Profile = serde_json::from_value(json).unwrap();
    assert_eq!(legacy.connection_type, "ssh");
    let mut p = legacy;
    p.connection_type = "sftp".into();
    p.validate().unwrap();
    let mut snapshot = Snapshot::default();
    snapshot.profiles.push(p.clone());
    let encoded = backup::encode(&snapshot, "fixture-backup-password", false).unwrap();
    let decoded = backup::decode(&encoded, "fixture-backup-password").unwrap();
    let restored: backup::Backup = serde_json::from_slice(&decoded).unwrap();
    assert_eq!(restored.snapshot.profiles[0].connection_type, "sftp");
    p.connection_type = "ftp".into();
    assert!(p.validate().is_err());
}

#[test]
#[cfg(windows)]
fn password_persistence_backup_portability_and_removal() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.db");
    let store = Store::open(&path).unwrap();
    let p = profile();
    let secret = "fixture-비밀-password-91!";
    store
        .save_profile(p.clone(), Some(Zeroizing::new(secret.into())), true)
        .unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    let original = store.read().unwrap();
    assert_eq!(
        store.password(&original.profiles[0]).unwrap().as_str(),
        secret
    );
    assert!(!String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains(secret));
    let backup_pw = "fixture-portable-backup";
    let encrypted = backup::encode(&original, backup_pw, true).unwrap();
    let file = directory.path().join("fixture.nxtbackup");
    backup::write_file(&file, &encrypted).unwrap();
    assert!(backup::read_file(&file, "wrong-password-fixture").is_err());
    assert!(!String::from_utf8_lossy(&encrypted).contains(secret));
    let target = Store::open(&directory.path().join("target.db")).unwrap();
    let plain = backup::read_file(&file, backup_pw).unwrap();
    let restored = backup::restore(&target, &plain, "replace").unwrap();
    assert_eq!(
        target.password(&restored.profiles[0]).unwrap().as_str(),
        secret
    );
    assert_ne!(
        restored.profiles[0].secret_ref,
        original.profiles[0].secret_ref
    );
    let no_secrets = backup::encode(&original, backup_pw, false).unwrap();
    backup::write_file(&file, &no_secrets).unwrap();
    assert!(backup::write_file(&directory.path().join("settings.db"), &no_secrets).is_err());
    let restored = backup::restore(
        &target,
        &backup::decode(&no_secrets, backup_pw).unwrap(),
        "replace",
    )
    .unwrap();
    assert!(restored.secrets.is_empty());
    assert!(restored.profiles[0].secret_ref.is_none());
    let mut edited = original.profiles[0].clone();
    edited.name = "수정됨".into();
    store.save_profile(edited.clone(), None, true).unwrap();
    assert_eq!(
        store
            .password(&store.read().unwrap().profiles[0])
            .unwrap()
            .as_str(),
        secret
    );
    store.save_profile(edited, None, false).unwrap();
    assert!(store.read().unwrap().secrets.is_empty());
    store.delete_profile(&p.id).unwrap();
    assert!(store.read().unwrap().profiles.is_empty());
}
#[test]
fn failed_restore_and_transaction_preserve_existing_data() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("state.db")).unwrap();
    store.save_profile(profile(), None, false).unwrap();
    let before = serde_json::to_vec(&store.read().unwrap()).unwrap();
    assert!(
        store
            .update(|doc| {
                doc.profiles.clear();
                Err("injected failure".into())
            })
            .is_err()
    );
    assert_eq!(before, serde_json::to_vec(&store.read().unwrap()).unwrap());
    let mut invalid = Snapshot::default();
    let mut p = profile();
    p.port = 0;
    invalid.profiles.push(p);
    let malformed = serde_json::to_vec(&backup::Backup {
        version: 1,
        snapshot: invalid,
        passwords: Default::default(),
    })
    .unwrap();
    assert!(backup::restore(&store, &malformed, "replace").is_err());
    assert_eq!(before, serde_json::to_vec(&store.read().unwrap()).unwrap());
    let doc = store.read().unwrap();
    let blob = backup::encode(&doc, "fixture-backup-password", false).unwrap();
    let plain = backup::decode(&blob, "fixture-backup-password").unwrap();
    let merged = backup::restore(&store, &plain, "merge").unwrap();
    assert_eq!(merged.profiles.len(), 2);
    assert_ne!(merged.profiles[0].id, merged.profiles[1].id);
}

#[test]
fn host_key_algorithm_legacy_default_validation_and_persistence() {
    let mut value = serde_json::to_value(profile()).unwrap();
    value.as_object_mut().unwrap().remove("hostKeyAlgorithm");
    let mut legacy: Profile = serde_json::from_value(value).unwrap();
    assert_eq!(legacy.host_key_algorithm, "ssh-rsa");
    legacy.validate().unwrap();
    legacy.host_key_algorithm = "ssh-dss".into();
    assert!(legacy.validate().is_err());
    legacy.host_key_algorithm = "rsa-sha2-512".into();
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("settings.db");
    let store = Store::open(&database).unwrap();
    store
        .update(|doc| {
            doc.profiles.push(legacy);
            Ok(())
        })
        .unwrap();
    drop(store);
    assert_eq!(
        Store::open(&database).unwrap().read().unwrap().profiles[0].host_key_algorithm,
        "rsa-sha2-512"
    );
}
