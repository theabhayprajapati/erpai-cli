use crate::cli::Global;
use crate::error::{CliError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_BASE_URL: &str = "https://apps.erp.ai";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    #[serde(default)]
    pub key_id: Option<String>,
    #[serde(default)]
    pub key_name: Option<String>,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub org_name: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub allowed_apps: Vec<String>,
    #[serde(default)]
    pub created_at: String,
}

impl Profile {
    pub fn new(name: &str, base_url: &str, api_key: &str) -> Self {
        Self {
            name: name.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            key_id: None,
            key_name: None,
            org_id: None,
            org_name: None,
            user_id: None,
            email: None,
            scopes: vec![],
            allowed_apps: vec![],
            created_at: now_unix(),
        }
    }

    /// Identity summary safe to print (never the key itself).
    pub fn identity(&self) -> serde_json::Value {
        serde_json::json!({
            "profile": self.name,
            "baseUrl": self.base_url,
            "org": { "id": self.org_id, "name": self.org_name },
            "user": { "id": self.user_id, "email": self.email },
            "apiKey": {
                "id": self.key_id,
                "name": self.key_name,
                "scopes": self.scopes,
                "allowedApps": self.allowed_apps,
                "prefix": self.api_key.chars().take(12).collect::<String>(),
            },
        })
    }
}

fn now_unix() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

pub struct ProfileStore {
    dir: PathBuf,
}

impl ProfileStore {
    pub fn open() -> Result<Self> {
        let dir = if let Ok(h) = std::env::var("ERPAI_CONFIG_HOME") {
            PathBuf::from(h)
        } else if let Ok(x) = std::env::var("XDG_CONFIG_HOME") {
            PathBuf::from(x).join("erpai")
        } else if let Some(b) = directories::BaseDirs::new() {
            b.home_dir().join(".config").join("erpai")
        } else {
            return Err(CliError::internal(
                "cannot resolve a config dir (set ERPAI_CONFIG_HOME)",
            ));
        };
        Ok(Self::at(dir))
    }
    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, name: &str) -> Result<PathBuf> {
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(CliError::validation(format!(
                "invalid profile name '{name}' (use letters, digits, - or _)"
            )));
        }
        Ok(self.dir.join("profiles").join(format!("{name}.json")))
    }

    pub fn load(&self, name: &str) -> Result<Option<Profile>> {
        let p = self.path(name)?;
        match std::fs::read(&p) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes).map_err(|e| {
                CliError::internal(format!("corrupt profile {}: {e}", p.display()))
            })?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, profile: &Profile) -> Result<()> {
        let p = self.path(&profile.name)?;
        let parent = p.parent().expect("profiles dir");
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
        let tmp = parent.join(format!(".{}.tmp", profile.name));
        std::fs::write(&tmp, serde_json::to_vec_pretty(profile)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, &p)?;
        Ok(())
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        let p = self.path(name)?;
        match std::fs::remove_file(p) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn list(&self) -> Result<Vec<String>> {
        let mut out = vec![];
        if let Ok(rd) = std::fs::read_dir(self.dir.join("profiles")) {
            for e in rd.flatten() {
                if let Some(n) = e.file_name().to_str().and_then(|s| s.strip_suffix(".json")) {
                    if !n.starts_with('.') {
                        out.push(n.to_string());
                    }
                }
            }
        }
        out.sort();
        Ok(out)
    }
}

pub fn require_profile(g: &Global) -> Result<Profile> {
    ProfileStore::open()?.load(&g.profile)?.ok_or_else(|| {
        CliError::auth(format!("no credentials for profile '{}'", g.profile))
            .with_hint("run `erpai login` (or `erpai login --api-key` with an existing key)")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_permissions() {
        let d = tempfile::tempdir().unwrap();
        let s = ProfileStore::at(d.path().to_path_buf());
        let p = Profile::new("default", "https://apps.erp.ai", "erp_pat_live_x");
        s.save(&p).unwrap();
        let back = s.load("default").unwrap().unwrap();
        assert_eq!(back.api_key, "erp_pat_live_x");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(d.path().join("profiles/default.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        assert_eq!(s.list().unwrap(), vec!["default".to_string()]);
        s.delete("default").unwrap();
        assert!(s.load("default").unwrap().is_none());
    }

    #[test]
    fn rejects_bad_profile_names() {
        let d = tempfile::tempdir().unwrap();
        let s = ProfileStore::at(d.path().to_path_buf());
        assert!(s.load("../etc").is_err());
    }

    #[test]
    fn identity_never_contains_the_key() {
        let p = Profile::new(
            "default",
            "https://apps.erp.ai",
            "erp_pat_live_secretsecretsecret",
        );
        let s = p.identity().to_string();
        assert!(!s.contains("secretsecret"));
        assert!(s.contains("erp_pat_live"));
    }
}
