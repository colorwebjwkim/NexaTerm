use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub group: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub auth_method: String,
    pub key_path: String,
    pub secret_ref: Option<String>,
    #[serde(default = "default_connection_type")]
    pub connection_type: String,
    #[serde(default = "default_host_key_algorithm")]
    pub host_key_algorithm: String,
}
fn default_host_key_algorithm() -> String {
    "ssh-rsa".into()
}
fn default_connection_type() -> String {
    "ssh".into()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub theme: String,
    pub font_size: u16,
    pub scrollback: u32,
    #[serde(default = "default_confirm_exit")]
    pub confirm_exit: bool,
}
fn default_confirm_exit() -> bool {
    true
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            font_size: 14,
            scrollback: 5000,
            confirm_exit: true,
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Snapshot {
    pub profiles: Vec<Profile>,
    pub settings: Settings,
    pub secrets: HashMap<String, String>,
    pub trusted_keys: HashMap<String, String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub profiles: Vec<Profile>,
    pub settings: Settings,
}
impl Snapshot {
    pub fn view(&self) -> View {
        View {
            profiles: self.profiles.clone(),
            settings: self.settings.clone(),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.profiles.len() > 1000 || self.secrets.len() > 1000 || self.trusted_keys.len() > 2000
        {
            return Err("항목 수 제한을 초과했습니다".into());
        }
        if !["dark", "light"].contains(&self.settings.theme.as_str())
            || !(10..=28).contains(&self.settings.font_size)
            || !(100..=20000).contains(&self.settings.scrollback)
        {
            return Err("설정 값이 올바르지 않습니다".into());
        }
        let mut ids = HashSet::new();
        for p in &self.profiles {
            p.validate()?;
            if let Some(reference) = &p.secret_ref
                && !self.secrets.contains_key(reference)
            {
                return Err("자격 증명 참조를 찾을 수 없습니다".into());
            }
            if !ids.insert(&p.id) {
                return Err("중복 프로필 ID".into());
            }
        }
        for (k, v) in &self.secrets {
            if uuid::Uuid::parse_str(k).is_err() || v.len() > 65536 {
                return Err("자격 증명 형식이 올바르지 않습니다".into());
            }
        }
        Ok(())
    }
}
impl Profile {
    pub fn validate(&self) -> Result<(), String> {
        if ![
            "ssh-rsa",
            "auto",
            "rsa-sha2-512",
            "rsa-sha2-256",
            "ssh-ed25519",
            "ecdsa-sha2-nistp256",
            "ecdsa-sha2-nistp384",
            "ecdsa-sha2-nistp521",
        ]
        .contains(&self.host_key_algorithm.as_str())
        {
            return Err("호스트키 알고리즘이 올바르지 않습니다".into());
        }
        if !["ssh", "sftp"].contains(&self.connection_type.as_str()) {
            return Err("접속 방식이 올바르지 않습니다".into());
        }
        if uuid::Uuid::parse_str(&self.id).is_err()
            || self.name.trim().is_empty()
            || self.name.len() > 160
            || self.host.trim().is_empty()
            || self.host.len() > 253
            || self.port == 0
            || self.username.trim().is_empty()
            || self.username.len() > 128
            || self.group.len() > 160
            || self.tags.len() > 32
            || self.tags.iter().any(|x| x.len() > 80)
            || self.key_path.len() > 4096
            || !["password", "privateKey"].contains(&self.auth_method.as_str())
            || self
                .host
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || c == '/' || c == '\\')
            || self.username.chars().any(|c| c.is_control())
        {
            return Err("프로필 입력을 확인하세요".into());
        }
        if let Some(r) = &self.secret_ref
            && uuid::Uuid::parse_str(r).is_err()
        {
            return Err("자격 증명 참조가 올바르지 않습니다".into());
        }
        Ok(())
    }
}
