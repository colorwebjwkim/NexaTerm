use crate::{
    models::{Profile, Settings, Snapshot},
    secrets,
};
use rusqlite::{Connection, OptionalExtension};
use std::{path::Path, sync::Mutex};
use zeroize::Zeroizing;

pub struct Store {
    db: Mutex<Connection>,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        let db = Connection::open(path).map_err(|_| "설정 DB를 열 수 없습니다")?;
        db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA secure_delete=ON; CREATE TABLE IF NOT EXISTS app_state (id INTEGER PRIMARY KEY CHECK(id=1), document TEXT NOT NULL);").map_err(|_| "설정 DB 초기화 실패")?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS local_browser_location (id INTEGER PRIMARY KEY CHECK(id=1), root TEXT NOT NULL, path TEXT NOT NULL);").map_err(|_| "로컬 위치 설정 초기화 실패")?;
        db.execute(
            "INSERT OR IGNORE INTO app_state VALUES(1, ?1)",
            [serde_json::to_string(&Snapshot::default()).unwrap()],
        )
        .map_err(|_| "설정 생성 실패")?;
        Ok(Self { db: Mutex::new(db) })
    }
    pub fn local_location(&self) -> Result<Option<(String, String)>, String> {
        let db = self.db.lock().map_err(|_| "설정 잠금 오류")?;
        db.query_row(
            "SELECT root, path FROM local_browser_location WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| "로컬 위치 읽기 실패".into())
    }
    pub fn save_local_location(&self, root: &str, path: &str) -> Result<(), String> {
        let db = self.db.lock().map_err(|_| "설정 잠금 오류")?;
        db.execute("INSERT INTO local_browser_location (id,root,path) VALUES (1,?1,?2) ON CONFLICT(id) DO UPDATE SET root=excluded.root,path=excluded.path WHERE root<>excluded.root OR path<>excluded.path", rusqlite::params![root,path])
            .map_err(|_| "로컬 위치 저장 실패")?;
        Ok(())
    }
    pub fn read(&self) -> Result<Snapshot, String> {
        let db = self.db.lock().map_err(|_| "설정 잠금 오류")?;
        Self::read_db(&db)
    }
    fn read_db(db: &Connection) -> Result<Snapshot, String> {
        let json: String = db
            .query_row("SELECT document FROM app_state WHERE id=1", [], |r| {
                r.get(0)
            })
            .map_err(|_| "설정 읽기 실패")?;
        let doc: Snapshot = serde_json::from_str(&json).map_err(|_| "설정 형식 오류")?;
        doc.validate()?;
        Ok(doc)
    }
    pub fn update(
        &self,
        f: impl FnOnce(&mut Snapshot) -> Result<(), String>,
    ) -> Result<Snapshot, String> {
        let mut db = self.db.lock().map_err(|_| "설정 잠금 오류")?;
        let tx = db.transaction().map_err(|_| "설정 트랜잭션 실패")?;
        let mut doc = Self::read_db(&tx)?;
        f(&mut doc)?;
        doc.validate()?;
        tx.execute(
            "UPDATE app_state SET document=?1 WHERE id=1",
            [serde_json::to_string(&doc).map_err(|_| "설정 변환 실패")?],
        )
        .map_err(|_| "설정 저장 실패")?;
        tx.commit().map_err(|_| "설정 저장 완료 실패")?;
        Ok(doc)
    }
    pub fn save_profile(
        &self,
        mut profile: Profile,
        password: Option<Zeroizing<String>>,
        save_secret: bool,
    ) -> Result<Snapshot, String> {
        profile.secret_ref = None;
        profile.validate()?;
        self.update(|doc| {
            let old = doc.profiles.iter().find(|p| p.id == profile.id).cloned();
            if save_secret {
                if let Some(value) = password.filter(|v| !v.is_empty()) {
                    let reference = uuid::Uuid::new_v4().to_string();
                    doc.secrets
                        .insert(reference.clone(), secrets::protect(&value)?);
                    profile.secret_ref = Some(reference);
                } else {
                    profile.secret_ref = old
                        .as_ref()
                        .filter(|p| p.auth_method == profile.auth_method)
                        .and_then(|p| p.secret_ref.clone());
                    if profile.secret_ref.is_none() {
                        return Err("저장할 비밀번호 또는 passphrase를 입력하세요".into());
                    }
                }
            }
            if let Some(reference) = old.and_then(|p| p.secret_ref)
                && profile.secret_ref.as_ref() != Some(&reference)
            {
                doc.secrets.remove(&reference);
            }
            doc.profiles.retain(|p| p.id != profile.id);
            doc.profiles.push(profile);
            Ok(())
        })
    }
    pub fn delete_profile(&self, id: &str) -> Result<Snapshot, String> {
        self.update(|doc| {
            if let Some(p) = doc.profiles.iter().find(|p| p.id == id)
                && let Some(r) = &p.secret_ref
            {
                doc.secrets.remove(r);
            }
            doc.profiles.retain(|p| p.id != id);
            Ok(())
        })
    }
    pub fn save_settings(&self, settings: Settings) -> Result<Snapshot, String> {
        self.update(|doc| {
            doc.settings = settings;
            Ok(())
        })
    }
    pub fn set_confirm_exit(&self, enabled: bool) -> Result<Snapshot, String> {
        self.update(|doc| {
            doc.settings.confirm_exit = enabled;
            Ok(())
        })
    }
    pub fn password(&self, profile: &Profile) -> Result<Zeroizing<String>, String> {
        let doc = self.read()?;
        let r = profile.secret_ref.as_ref().ok_or("비밀번호를 입력하세요")?;
        secrets::reveal(doc.secrets.get(r).ok_or("저장된 비밀번호가 없습니다")?)
    }
}
