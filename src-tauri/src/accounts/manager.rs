use super::microsoft::{self as ms, Endpoints};
use super::secrets::{self, SecretStore};
use super::store::{self, AccountKind, StoreData, StoredAccount};
use super::yggdrasil as ygg;
use super::{AccountError, AccountInfo, AuthSession, AuthlibInjectorConfig, Result, offline, undash};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// HTTP client for the account commands. Third-party login servers are user supplied and may
/// legitimately live on a LAN, but never on this machine or a link-local address, and a redirect
/// must not lead there either.
fn guarded_http() -> reqwest::Client {
    use crate::settings::guard::{GuardedResolver, HostPolicy, redirect_policy};
    reqwest::Client::builder()
        .user_agent("pcl-tauri-port")
        .timeout(std::time::Duration::from_secs(30))
        .redirect(redirect_policy(HostPolicy::AllowLan))
        .dns_resolver(Arc::new(GuardedResolver(HostPolicy::AllowLan)))
        .build()
        .expect("http client")
}

pub struct AccountManager {
    dir: PathBuf,
    secrets: Arc<dyn SecretStore>,
    data: Mutex<StoreData>,
    cancel: Arc<AtomicBool>,
    pub endpoints: Endpoints,
    http: reqwest::Client,
}

impl AccountManager {
    pub fn new(dir: PathBuf, secrets: Arc<dyn SecretStore>) -> Result<Self> {
        let data = store::load(&dir)?;
        Ok(Self {
            dir,
            secrets,
            data: Mutex::new(data),
            cancel: Arc::new(AtomicBool::new(false)),
            endpoints: Endpoints::default(),
            http: guarded_http(),
        })
    }

    fn persist(&self, d: &StoreData) -> Result<()> {
        store::save(&self.dir, d)
    }

    pub fn list(&self) -> Vec<AccountInfo> {
        self.data.lock().unwrap().accounts.iter().map(|a| a.info()).collect()
    }

    pub fn selected(&self) -> Option<String> {
        self.data.lock().unwrap().selected.clone()
    }

    pub fn select(&self, id: &str) -> Result<()> {
        let mut d = self.data.lock().unwrap();
        if !d.accounts.iter().any(|a| a.id == id) {
            return Err(AccountError::NotFound(id.into()));
        }
        d.selected = Some(id.into());
        self.persist(&d)
    }

    pub fn client_id(&self) -> Result<String> {
        let s = self.data.lock().unwrap().settings.ms_client_id.clone();
        ms::resolve_client_id(s.as_deref())
    }

    pub fn set_client_id(&self, id: Option<String>) -> Result<()> {
        let id = id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        // Azure application (client) IDs are GUIDs. Anything else is a typo or an injection attempt.
        if let Some(i) = &id {
            uuid::Uuid::parse_str(i)
                .map_err(|_| AccountError::Invalid("Microsoft 客户端 ID 必须是 GUID（xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx）".into()))?;
        }
        let mut d = self.data.lock().unwrap();
        d.settings.ms_client_id = id;
        self.persist(&d)
    }

    fn add(&self, kind: AccountKind) -> Result<AccountInfo> {
        let acc = StoredAccount { id: uuid::Uuid::new_v4().to_string(), kind };
        let mut d = self.data.lock().unwrap();
        let info = acc.info();
        d.accounts.push(acc);
        d.selected.get_or_insert_with(|| info.id.clone());
        self.persist(&d)?;
        Ok(info)
    }

    fn get(&self, id: &str) -> Result<StoredAccount> {
        self.data.lock().unwrap().accounts.iter().find(|a| a.id == id).cloned().ok_or_else(|| AccountError::NotFound(id.into()))
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let mut d = self.data.lock().unwrap();
        let before = d.accounts.len();
        d.accounts.retain(|a| a.id != id);
        if d.accounts.len() == before {
            return Err(AccountError::NotFound(id.into()));
        }
        if d.selected.as_deref() == Some(id) {
            d.selected = d.accounts.first().map(|a| a.id.clone());
        }
        self.persist(&d)?;
        drop(d);
        for k in [secrets::ms_refresh_key(id), secrets::ygg_access_key(id), secrets::ygg_client_key(id)] {
            self.secrets.delete(&k)?;
        }
        Ok(())
    }

    // ---- offline ----
    pub fn add_offline(&self, name: &str) -> Result<AccountInfo> {
        offline::validate_name(name)?;
        self.add(AccountKind::Offline { name: name.into(), uuid: offline::offline_uuid(name) })
    }

    // ---- microsoft ----
    pub async fn ms_begin(&self) -> Result<(String, ms::DeviceCode)> {
        let cid = self.client_id()?;
        self.cancel.store(false, Ordering::SeqCst);
        let dc = ms::request_device_code(&self.http, &self.endpoints, &cid).await?;
        Ok((cid, dc))
    }

    pub async fn ms_finish(&self, cid: &str, dc: &ms::DeviceCode) -> Result<AccountInfo> {
        let cancel = self.cancel.clone();
        let tokens = ms::poll_for_tokens(&self.http, &self.endpoints, cid, dc, move || cancel.load(Ordering::SeqCst)).await?;
        let login = ms::minecraft_from_ms(&self.http, &self.endpoints, &tokens.access_token).await?;
        let dashed = super::dash(&login.profile.uuid);
        // Re-login of an existing account updates it in place.
        let existing = self.data.lock().unwrap().accounts.iter().find_map(|a| match &a.kind {
            AccountKind::Microsoft { uuid, .. } if *uuid == dashed => Some(a.id.clone()),
            _ => None,
        });
        let info = match existing {
            Some(id) => {
                let mut d = self.data.lock().unwrap();
                if let Some(a) = d.accounts.iter_mut().find(|a| a.id == id) {
                    a.kind = AccountKind::Microsoft { name: login.profile.name.clone(), uuid: dashed };
                }
                self.persist(&d)?;
                self.get(&id)?.info()
            }
            None => self.add(AccountKind::Microsoft { name: login.profile.name.clone(), uuid: dashed })?,
        };
        self.secrets.set(&secrets::ms_refresh_key(&info.id), &tokens.refresh_token)?;
        Ok(info)
    }

    pub fn cancel_login(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    // ---- yggdrasil ----
    pub async fn server_info(&self, url: &str) -> Result<ygg::ServerInfo> {
        ygg::checked_server_url(url)?;
        ygg::discover(&self.http, url).await
    }

    /// Returns the added accounts (one per available profile; the server's selected profile first).
    pub async fn add_yggdrasil(&self, server: &str, identifier: &str, password: &str) -> Result<Vec<AccountInfo>> {
        ygg::checked_server_url(server)?;
        let info = ygg::discover(&self.http, server).await?;
        // `discover` may have followed the server's API-location hint: check where the password really goes.
        ygg::checked_server_url(&info.url)?;
        ygg::ensure_secure_for_credentials(&info.url)?;
        let res = ygg::authenticate(&self.http, &info.url, identifier, password, None).await?;
        let mut profiles = Vec::new();
        if let Some(p) = res.profile.clone() {
            profiles.push(p);
        }
        for p in &res.available {
            if !profiles.iter().any(|x| x.id == p.id) {
                profiles.push(p.clone());
            }
        }
        if profiles.is_empty() {
            return Err(AccountError::Server("this account has no profile on the server".into()));
        }
        let mut out = Vec::new();
        for p in profiles {
            let a = self.add(AccountKind::Yggdrasil {
                name: p.name.clone(),
                uuid: ygg::dashed(&p),
                server_url: info.url.clone(),
                server_name: info.name.clone(),
                identifier: identifier.into(),
            })?;
            self.secrets.set(&secrets::ygg_access_key(&a.id), &res.access_token)?;
            self.secrets.set(&secrets::ygg_client_key(&a.id), &res.client_token)?;
            out.push(a);
        }
        Ok(out)
    }

    // ---- session ----
    /// Produce launch credentials. Refreshes tokens as needed.
    pub async fn session(&self, id: &str) -> Result<AuthSession> {
        let acc = self.get(id)?;
        match acc.kind {
            AccountKind::Offline { name, uuid } => Ok(AuthSession {
                player_name: name,
                uuid: undash(&uuid),
                access_token: undash(&uuid), // vanilla ignores it offline
                user_type: "legacy".into(),
                account_kind: "offline".into(),
                client_token: None,
                authlib_injector: None,
            }),
            AccountKind::Microsoft { .. } => {
                let cid = self.client_id()?;
                let rt = self.secrets.get(&secrets::ms_refresh_key(id))?.ok_or(AccountError::NeedRelogin)?;
                let t = ms::refresh(&self.http, &self.endpoints, &cid, &rt).await?;
                // Microsoft rotates refresh tokens; persist the new one before anything else.
                self.secrets.set(&secrets::ms_refresh_key(id), &t.refresh_token)?;
                let login = ms::minecraft_from_ms(&self.http, &self.endpoints, &t.access_token).await?;
                let dashed = super::dash(&login.profile.uuid);
                {
                    // Keep the stored name current (renames).
                    let mut d = self.data.lock().unwrap();
                    if let Some(a) = d.accounts.iter_mut().find(|a| a.id == id) {
                        a.kind = AccountKind::Microsoft { name: login.profile.name.clone(), uuid: dashed };
                    }
                    self.persist(&d)?;
                }
                Ok(AuthSession {
                    player_name: login.profile.name,
                    uuid: login.profile.uuid,
                    access_token: login.access_token,
                    user_type: "msa".into(),
                    account_kind: "microsoft".into(),
                    client_token: None,
                    authlib_injector: None,
                })
            }
            AccountKind::Yggdrasil { name, uuid, server_url, .. } => {
                let mut access = self.secrets.get(&secrets::ygg_access_key(id))?.ok_or(AccountError::NeedRelogin)?;
                let client = self.secrets.get(&secrets::ygg_client_key(id))?.ok_or(AccountError::NeedRelogin)?;
                if !ygg::validate(&self.http, &server_url, &access, &client).await? {
                    let r = ygg::refresh(&self.http, &server_url, &access, &client).await.map_err(|_| AccountError::NeedRelogin)?;
                    access = r.access_token;
                    self.secrets.set(&secrets::ygg_access_key(id), &access)?;
                }
                let prefetched = ygg::discover(&self.http, &server_url).await.ok().map(|i| i.prefetched);
                Ok(AuthSession {
                    player_name: name,
                    uuid: undash(&uuid),
                    access_token: access,
                    user_type: "mojang".into(),
                    account_kind: "yggdrasil".into(),
                    client_token: Some(client),
                    authlib_injector: Some(AuthlibInjectorConfig { server_url, prefetched }),
                })
            }
        }
    }

    /// Skin PNG as a data URL for the account (None for offline / no custom skin).
    pub async fn skin(&self, id: &str) -> Result<Option<String>> {
        let acc = self.get(id)?;
        let (uuid, base) = match &acc.kind {
            AccountKind::Offline { .. } => return Ok(None),
            AccountKind::Microsoft { uuid, .. } => (uuid.clone(), super::skin::MOJANG_SESSION_PROFILE.to_string()),
            AccountKind::Yggdrasil { uuid, server_url, .. } => {
                (uuid.clone(), format!("{server_url}/sessionserver/session/minecraft/profile"))
            }
        };
        let info = super::skin::fetch_skin_info(&self.http, &base, &uuid).await?;
        match info.url {
            Some(u) => Ok(Some(super::skin::skin_data_url(&self.http, &u).await?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrets::MemoryStore;

    fn mgr() -> (AccountManager, PathBuf) {
        let dir = std::env::temp_dir().join(format!("pcl-mgr-{}", uuid::Uuid::new_v4()));
        (AccountManager::new(dir.clone(), Arc::new(MemoryStore::default())).unwrap(), dir)
    }

    #[tokio::test]
    async fn offline_lifecycle() {
        let (m, dir) = mgr();
        let a = m.add_offline("Notch").unwrap();
        assert_eq!(m.selected().as_deref(), Some(a.id.as_str()));
        let s = m.session(&a.id).await.unwrap();
        assert_eq!(s.uuid, "b50ad385829d3141a2167e7d7539ba7f");
        assert_eq!(s.user_type, "legacy");
        assert!(m.add_offline("bad name").is_err());
        // Reload from disk.
        let m2 = AccountManager::new(dir.clone(), Arc::new(MemoryStore::default())).unwrap();
        assert_eq!(m2.list().len(), 1);
        m.remove(&a.id).unwrap();
        assert!(m.list().is_empty() && m.selected().is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn microsoft_requires_client_id() {
        let (m, dir) = mgr();
        if std::env::var("PCL_MS_CLIENT_ID").is_err() && option_env!("PCL_MS_CLIENT_ID").is_none() {
            assert!(matches!(m.client_id(), Err(AccountError::MissingClientId)));
        }
        assert!(m.set_client_id(Some("abc".into())).is_err(), "not a GUID");
        assert!(m.set_client_id(Some("1111 2222".into())).is_err());
        let cid = "00000000-1111-2222-3333-444444444444";
        m.set_client_id(Some(cid.into())).unwrap();
        assert_eq!(m.client_id().unwrap(), cid);
        m.set_client_id(None).unwrap();
        std::fs::remove_dir_all(dir).ok();
    }
}
