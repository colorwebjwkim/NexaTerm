use crate::{models::Snapshot, secrets, store::Store};
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{
    io::{Read, Write},
    path::Path,
};
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"NXTBKP01";
pub const MAX_BACKUP: usize = 16 * 1024 * 1024;
pub fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.extension().and_then(|s| s.to_str()) != Some("nxtbackup") {
        return Err("백업 파일 확장자는 .nxtbackup이어야 합니다".into());
    }
    let directory = path.parent().ok_or("백업 저장 폴더 오류")?;
    let mut temp =
        tempfile::NamedTempFile::new_in(directory).map_err(|_| "백업 임시 파일 생성 실패")?;
    temp.write_all(bytes).map_err(|_| "백업 쓰기 실패")?;
    temp.as_file()
        .sync_all()
        .map_err(|_| "백업 파일 저장 완료 실패")?;
    temp.persist(path).map_err(|_| "백업 파일 교체 실패")?;
    Ok(())
}
pub fn read_file(path: &Path, password: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    let file = std::fs::File::open(path).map_err(|_| "백업 파일을 읽을 수 없습니다")?;
    if file.metadata().map_err(|_| "백업 크기 확인 실패")?.len() > MAX_BACKUP as u64 {
        return Err("백업 크기 제한 초과".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BACKUP as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "백업 읽기 실패")?;
    decode(&bytes, password)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Backup {
    pub version: u8,
    pub snapshot: Snapshot,
    pub passwords: HashMap<String, String>,
}
impl Drop for Backup {
    fn drop(&mut self) {
        for value in self.passwords.values_mut() {
            zeroize::Zeroize::zeroize(value);
        }
    }
}
fn derive(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, String> {
    if password.len() < 12 || password.len() > 1024 {
        return Err("백업 암호는 12~1024바이트로 입력하세요".into());
    }
    let mut key = Zeroizing::new([0u8; 32]);
    let params = Params::new(65536, 3, 1, Some(32)).map_err(|_| "암호화 설정 오류")?;
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut *key)
        .map_err(|_| "백업 키 생성 실패")?;
    Ok(key)
}
pub fn encode(doc: &Snapshot, password: &str, include_secrets: bool) -> Result<Vec<u8>, String> {
    doc.validate()?;
    let mut clean = doc.clone();
    clean.secrets.clear();
    clean.trusted_keys.clear();
    let mut passwords = HashMap::new();
    for p in &mut clean.profiles {
        if include_secrets && let Some(r) = &p.secret_ref {
            let secret = secrets::reveal(doc.secrets.get(r).ok_or("자격 증명이 없습니다")?)?;
            passwords.insert(p.id.clone(), secret.to_string());
        }
        p.secret_ref = None;
    }
    let payload = Backup {
        version: 1,
        snapshot: clean,
        passwords,
    };
    let json = Zeroizing::new(serde_json::to_vec(&payload).map_err(|_| "백업 변환 실패")?);
    // Explicitly clear temporary secret strings before encrypting the zeroizing buffer.
    let mut payload = payload;
    for v in payload.passwords.values_mut() {
        zeroize::Zeroize::zeroize(v);
    }
    if json.len() > MAX_BACKUP - 64 {
        return Err("백업 크기 제한 초과".into());
    }
    let mut header = Vec::from(MAGIC.as_slice());
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    header.extend_from_slice(&salt);
    header.extend_from_slice(&nonce);
    let key = derive(password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&*key).map_err(|_| "백업 암호화 오류")?;
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &json,
                aad: &header,
            },
        )
        .map_err(|_| "백업 암호화 실패")?;
    header.extend(encrypted);
    Ok(header)
}
pub fn decode(bytes: &[u8], password: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    if bytes.len() < 52 || bytes.len() > MAX_BACKUP || &bytes[..8] != MAGIC {
        return Err("지원하지 않는 백업 또는 크기 오류".into());
    }
    let key = derive(password, &bytes[8..24])?;
    let cipher = Aes256Gcm::new_from_slice(&*key).map_err(|_| "백업 복호화 오류")?;
    let json = Zeroizing::new(
        cipher
            .decrypt(
                Nonce::from_slice(&bytes[24..36]),
                Payload {
                    msg: &bytes[36..],
                    aad: &bytes[..36],
                },
            )
            .map_err(|_| "백업 암호가 틀리거나 파일이 변조되었습니다")?,
    );
    let payload: Backup = serde_json::from_slice(&json).map_err(|_| "백업 내용 형식 오류")?;
    validate(&payload)?;
    Ok(json)
}
pub fn validate(payload: &Backup) -> Result<(), String> {
    if payload.version != 1
        || !payload.snapshot.secrets.is_empty()
        || !payload.snapshot.trusted_keys.is_empty()
        || payload
            .snapshot
            .profiles
            .iter()
            .any(|p| p.secret_ref.is_some())
        || payload.passwords.len() > 1000
    {
        return Err("백업 형식이 올바르지 않습니다".into());
    }
    payload.snapshot.validate()?;
    for (id, pw) in &payload.passwords {
        if pw.is_empty()
            || pw.len() > 16384
            || !payload.snapshot.profiles.iter().any(|p| &p.id == id)
        {
            return Err("백업 자격 증명 형식 오류".into());
        }
    }
    Ok(())
}
pub fn restore(store: &Store, json: &[u8], mode: &str) -> Result<Snapshot, String> {
    if mode != "merge" && mode != "replace" {
        return Err("복원 방식을 선택하세요".into());
    }
    let mut payload: Backup = serde_json::from_slice(json).map_err(|_| "백업 내용 형식 오류")?;
    validate(&payload)?;
    let result = store.update(|doc| {
        if mode == "replace" {
            doc.profiles.clear();
            doc.secrets.clear();
            doc.settings = payload.snapshot.settings.clone();
        }
        // Host-key trust is deliberately not imported or removed.
        for mut p in payload.snapshot.profiles.clone() {
            let old_id = p.id.clone();
            if doc.profiles.iter().any(|x| x.id == p.id) {
                p.id = uuid::Uuid::new_v4().to_string();
                p.name = format!("{} (복원)", p.name);
            }
            if let Some(password) = payload.passwords.get(&old_id) {
                let r = uuid::Uuid::new_v4().to_string();
                doc.secrets.insert(r.clone(), secrets::protect(password)?);
                p.secret_ref = Some(r);
            }
            doc.profiles.push(p);
        }
        Ok(())
    });
    for password in payload.passwords.values_mut() {
        zeroize::Zeroize::zeroize(password);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authenticated_backup_roundtrip_and_rejection() {
        let doc = Snapshot::default();
        let pw = "fixture-backup-password";
        let bytes = encode(&doc, pw, false).unwrap();
        assert!(decode(&bytes, pw).is_ok());
        assert!(decode(&bytes, "incorrect-password").is_err());
        let mut modified = bytes.clone();
        modified[30] ^= 1;
        assert!(decode(&modified, pw).is_err());
        assert_ne!(bytes, encode(&doc, pw, false).unwrap());
        assert!(decode(b"bad", pw).is_err());
    }
}
