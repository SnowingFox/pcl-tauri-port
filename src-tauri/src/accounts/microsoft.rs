//! Microsoft login: OAuth device code -> Xbox Live -> XSTS -> Minecraft services.
//!
//! The Azure app client ID is NOT bundled. Resolve it with [`resolve_client_id`]
//! (settings value, then `PCL_MS_CLIENT_ID` env, then build-time
//! `option_env!("PCL_MS_CLIENT_ID")`). See docs/api/accounts.md for how to register one.
use super::{AccountError, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

/// Placeholder only. Replace by configuration, never commit a real ID here
/// unless it is your own public app registration.
pub const CLIENT_ID_PLACEHOLDER: &str = "<REGISTER-YOUR-AZURE-APP-AND-SET-PCL_MS_CLIENT_ID>";
pub const SCOPE: &str = "XboxLive.signin offline_access";

pub fn resolve_client_id(setting: Option<&str>) -> Result<String> {
    let pick = |s: Option<&str>| s.map(str::trim).filter(|s| !s.is_empty() && *s != CLIENT_ID_PLACEHOLDER).map(String::from);
    pick(setting)
        .or_else(|| pick(std::env::var("PCL_MS_CLIENT_ID").ok().as_deref()))
        .or_else(|| pick(option_env!("PCL_MS_CLIENT_ID")))
        .ok_or(AccountError::MissingClientId)
}

#[derive(Debug, Clone)]
pub struct Endpoints {
    pub device_code: String,
    pub token: String,
    pub xbl: String,
    pub xsts: String,
    pub mc_login: String,
    pub entitlements: String,
    pub profile: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        let oauth = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
        Self {
            device_code: format!("{oauth}/devicecode"),
            token: format!("{oauth}/token"),
            xbl: "https://user.auth.xboxlive.com/user/authenticate".into(),
            xsts: "https://xsts.auth.xboxlive.com/xsts/authorize".into(),
            mc_login: "https://api.minecraftservices.com/authentication/login_with_xbox".into(),
            entitlements: "https://api.minecraftservices.com/entitlements/mcstore".into(),
            profile: "https://api.minecraftservices.com/minecraft/profile".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    #[serde(alias = "device_code")]
    pub device_code: String,
    #[serde(alias = "user_code")]
    pub user_code: String,
    #[serde(alias = "verification_uri")]
    pub verification_uri: String,
    #[serde(alias = "expires_in")]
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
    #[serde(default)]
    pub message: Option<String>,
}
fn default_interval() -> u64 {
    5
}

#[derive(Clone, PartialEq, Eq)]
pub struct MsTokens {
    pub access_token: String,
    pub refresh_token: String,
}

impl std::fmt::Debug for MsTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MsTokens").field("access_token", &"<redacted>").field("refresh_token", &"<redacted>").finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McProfile {
    pub uuid: String, // undashed
    pub name: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct McLogin {
    pub access_token: String,
    pub expires_in: u64,
    pub profile: McProfile,
}

impl std::fmt::Debug for McLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McLogin")
            .field("access_token", &"<redacted>")
            .field("expires_in", &self.expires_in)
            .field("profile", &self.profile)
            .finish()
    }
}

pub fn http() -> reqwest::Client {
    reqwest::Client::builder().user_agent("pcl-tauri-port").timeout(Duration::from_secs(30)).build().expect("http client")
}

pub async fn request_device_code(c: &reqwest::Client, ep: &Endpoints, client_id: &str) -> Result<DeviceCode> {
    let r = c.post(&ep.device_code).form(&[("client_id", client_id), ("scope", SCOPE)]).send().await?;
    let ok = r.status().is_success();
    let body = r.text().await?;
    if !ok {
        return Err(AccountError::BadResponse(oauth_error(&body)));
    }
    Ok(serde_json::from_str(&body)?)
}

fn oauth_error(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["error_description"].as_str().or(v["error"].as_str()).map(String::from))
        .unwrap_or_else(|| body.chars().take(200).collect())
}

#[derive(Debug, PartialEq, Eq)]
pub enum PollOutcome {
    Pending,
    SlowDown,
    Done(MsTokens),
}

fn parse_token_response(status_ok: bool, body: &str) -> Result<PollOutcome> {
    let v: serde_json::Value = serde_json::from_str(body)?;
    if status_ok {
        let at = v["access_token"].as_str().ok_or_else(|| AccountError::BadResponse("no access_token".into()))?;
        let rt = v["refresh_token"].as_str().ok_or_else(|| AccountError::BadResponse("no refresh_token".into()))?;
        return Ok(PollOutcome::Done(MsTokens { access_token: at.into(), refresh_token: rt.into() }));
    }
    match v["error"].as_str().unwrap_or("") {
        "authorization_pending" => Ok(PollOutcome::Pending),
        "slow_down" => Ok(PollOutcome::SlowDown),
        "authorization_declined" | "access_denied" => Err(AccountError::Declined),
        "expired_token" | "code_expired" => Err(AccountError::DeviceCodeExpired),
        "invalid_grant" | "interaction_required" => Err(AccountError::NeedRelogin),
        _ => Err(AccountError::BadResponse(oauth_error(body))),
    }
}

pub async fn poll_once(c: &reqwest::Client, ep: &Endpoints, client_id: &str, device_code: &str) -> Result<PollOutcome> {
    let r = c
        .post(&ep.token)
        .form(&[("grant_type", "urn:ietf:params:oauth:grant-type:device_code"), ("client_id", client_id), ("device_code", device_code)])
        .send()
        .await?;
    let ok = r.status().is_success();
    parse_token_response(ok, &r.text().await?)
}

/// Poll until done, expired, declined or cancelled. `cancelled` is checked each tick.
pub async fn poll_for_tokens(
    c: &reqwest::Client,
    ep: &Endpoints,
    client_id: &str,
    dc: &DeviceCode,
    cancelled: impl Fn() -> bool,
) -> Result<MsTokens> {
    let mut interval = dc.interval.max(1);
    let deadline = std::time::Instant::now() + Duration::from_secs(dc.expires_in);
    loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        if cancelled() {
            return Err(AccountError::Cancelled);
        }
        if std::time::Instant::now() > deadline {
            return Err(AccountError::DeviceCodeExpired);
        }
        match poll_once(c, ep, client_id, &dc.device_code).await? {
            PollOutcome::Done(t) => return Ok(t),
            PollOutcome::Pending => {}
            PollOutcome::SlowDown => interval += 5,
        }
    }
}

pub async fn refresh(c: &reqwest::Client, ep: &Endpoints, client_id: &str, refresh_token: &str) -> Result<MsTokens> {
    let r = c
        .post(&ep.token)
        .form(&[("grant_type", "refresh_token"), ("client_id", client_id), ("refresh_token", refresh_token), ("scope", SCOPE)])
        .send()
        .await?;
    let ok = r.status().is_success();
    match parse_token_response(ok, &r.text().await?)? {
        PollOutcome::Done(t) => Ok(t),
        _ => Err(AccountError::NeedRelogin),
    }
}

/// Returns (xbl token, user hash).
pub async fn xbox_live(c: &reqwest::Client, ep: &Endpoints, ms_access_token: &str) -> Result<(String, String)> {
    let r = c
        .post(&ep.xbl)
        .header("Accept", "application/json")
        .json(&json!({
            "Properties": {"AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={ms_access_token}")},
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await?;
    let ok = r.status().is_success();
    let body = r.text().await?;
    if !ok {
        return Err(AccountError::Xbox(format!("Xbox Live authentication failed: {}", body.chars().take(200).collect::<String>())));
    }
    parse_xbox_token(&body)
}

pub fn parse_xbox_token(body: &str) -> Result<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(body)?;
    let token = v["Token"].as_str().ok_or_else(|| AccountError::BadResponse("no Xbox Token".into()))?;
    let uhs = v["DisplayClaims"]["xui"][0]["uhs"].as_str().ok_or_else(|| AccountError::BadResponse("no uhs".into()))?;
    Ok((token.into(), uhs.into()))
}

/// Human message for known XSTS `XErr` codes.
pub fn xerr_message(code: u64) -> String {
    match code {
        2148916233 => "This Microsoft account has no Xbox account. Create one at xbox.com first.".into(),
        2148916235 => "Xbox Live is not available in your country/region.".into(),
        2148916236 | 2148916237 => "This account needs adult verification (Xbox page).".into(),
        2148916238 => "This is a child account; an adult must add it to a Microsoft family.".into(),
        c => format!("XSTS error {c}"),
    }
}

pub async fn xsts(c: &reqwest::Client, ep: &Endpoints, xbl_token: &str) -> Result<(String, String)> {
    let r = c
        .post(&ep.xsts)
        .header("Accept", "application/json")
        .json(&json!({
            "Properties": {"SandboxId": "RETAIL", "UserTokens": [xbl_token]},
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await?;
    let ok = r.status().is_success();
    let body = r.text().await?;
    if !ok {
        let code = serde_json::from_str::<serde_json::Value>(&body).ok().and_then(|v| v["XErr"].as_u64());
        return Err(AccountError::Xbox(
            code.map(xerr_message).unwrap_or_else(|| format!("XSTS failed: {}", body.chars().take(200).collect::<String>())),
        ));
    }
    parse_xbox_token(&body)
}

pub async fn minecraft_login(c: &reqwest::Client, ep: &Endpoints, uhs: &str, xsts_token: &str) -> Result<(String, u64)> {
    let r = c.post(&ep.mc_login).json(&json!({"identityToken": format!("XBL3.0 x={uhs};{xsts_token}")})).send().await?;
    let ok = r.status().is_success();
    let body = r.text().await?;
    if !ok {
        return Err(AccountError::BadResponse(format!("Minecraft login failed: {}", body.chars().take(200).collect::<String>())));
    }
    let v: serde_json::Value = serde_json::from_str(&body)?;
    let t = v["access_token"].as_str().ok_or_else(|| AccountError::BadResponse("no access_token".into()))?;
    Ok((t.into(), v["expires_in"].as_u64().unwrap_or(86400)))
}

/// True if the entitlements list contains the Java game.
pub fn owns_game(body: &str) -> Result<bool> {
    let v: serde_json::Value = serde_json::from_str(body)?;
    Ok(v["items"]
        .as_array()
        .map(|a| a.iter().any(|i| matches!(i["name"].as_str(), Some("product_minecraft") | Some("game_minecraft"))))
        .unwrap_or(false))
}

pub async fn check_ownership(c: &reqwest::Client, ep: &Endpoints, mc_token: &str) -> Result<bool> {
    let r = c.get(&ep.entitlements).bearer_auth(mc_token).send().await?;
    if !r.status().is_success() {
        return Err(AccountError::BadResponse(format!("entitlements check failed: HTTP {}", r.status())));
    }
    owns_game(&r.text().await?)
}

pub fn parse_profile(body: &str) -> Result<McProfile> {
    let v: serde_json::Value = serde_json::from_str(body)?;
    Ok(McProfile {
        uuid: v["id"].as_str().ok_or_else(|| AccountError::BadResponse("no profile id".into()))?.to_lowercase(),
        name: v["name"].as_str().ok_or_else(|| AccountError::BadResponse("no profile name".into()))?.into(),
    })
}

pub async fn fetch_profile(c: &reqwest::Client, ep: &Endpoints, mc_token: &str) -> Result<McProfile> {
    let r = c.get(&ep.profile).bearer_auth(mc_token).send().await?;
    if r.status().as_u16() == 404 {
        return Err(AccountError::NotOwned);
    }
    if !r.status().is_success() {
        return Err(AccountError::BadResponse(format!("profile fetch failed: HTTP {}", r.status())));
    }
    parse_profile(&r.text().await?)
}

/// MS access token -> Minecraft token + profile, with ownership check.
pub async fn minecraft_from_ms(c: &reqwest::Client, ep: &Endpoints, ms_access_token: &str) -> Result<McLogin> {
    let (xbl, _) = xbox_live(c, ep, ms_access_token).await?;
    let (xs, uhs) = xsts(c, ep, &xbl).await?;
    let (token, expires_in) = minecraft_login(c, ep, &uhs, &xs).await?;
    if !check_ownership(c, ep, &token).await? {
        return Err(AccountError::NotOwned);
    }
    let profile = fetch_profile(c, ep, &token).await?;
    Ok(McLogin { access_token: token, expires_in, profile })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn eps(s: &MockServer) -> Endpoints {
        let u = s.uri();
        Endpoints {
            device_code: format!("{u}/devicecode"),
            token: format!("{u}/token"),
            xbl: format!("{u}/xbl"),
            xsts: format!("{u}/xsts"),
            mc_login: format!("{u}/mclogin"),
            entitlements: format!("{u}/ent"),
            profile: format!("{u}/profile"),
        }
    }

    #[test]
    fn token_parsing() {
        assert_eq!(parse_token_response(false, r#"{"error":"authorization_pending"}"#).unwrap(), PollOutcome::Pending);
        assert_eq!(parse_token_response(false, r#"{"error":"slow_down"}"#).unwrap(), PollOutcome::SlowDown);
        assert!(matches!(parse_token_response(false, r#"{"error":"authorization_declined"}"#), Err(AccountError::Declined)));
        assert!(matches!(parse_token_response(false, r#"{"error":"expired_token"}"#), Err(AccountError::DeviceCodeExpired)));
        assert!(matches!(parse_token_response(false, r#"{"error":"invalid_grant"}"#), Err(AccountError::NeedRelogin)));
        let ok = parse_token_response(true, r#"{"access_token":"a","refresh_token":"r"}"#).unwrap();
        assert_eq!(ok, PollOutcome::Done(MsTokens { access_token: "a".into(), refresh_token: "r".into() }));
    }

    #[test]
    fn xbox_parsing_and_errors() {
        let (t, u) = parse_xbox_token(r#"{"Token":"T","DisplayClaims":{"xui":[{"uhs":"H"}]}}"#).unwrap();
        assert_eq!((t.as_str(), u.as_str()), ("T", "H"));
        assert!(xerr_message(2148916233).contains("no Xbox account"));
        assert!(xerr_message(2148916238).contains("child"));
    }

    #[test]
    fn ownership_and_profile() {
        assert!(owns_game(r#"{"items":[{"name":"game_minecraft"},{"name":"product_minecraft"}]}"#).unwrap());
        assert!(!owns_game(r#"{"items":[]}"#).unwrap());
        let p = parse_profile(r#"{"id":"B50AD385829D3141A2167E7D7539BA7F","name":"Notch"}"#).unwrap();
        assert_eq!(p.uuid, "b50ad385829d3141a2167e7d7539ba7f");
    }

    #[test]
    fn client_id_resolution() {
        assert_eq!(resolve_client_id(Some("abc")).unwrap(), "abc");
        assert!(resolve_client_id(Some(CLIENT_ID_PLACEHOLDER)).is_err() || std::env::var("PCL_MS_CLIENT_ID").is_ok());
    }

    #[tokio::test]
    async fn full_flow_mocked() {
        let s = MockServer::start().await;
        Mock::given(method("POST")).and(path("/devicecode")).respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code":"DC","user_code":"ABCD","verification_uri":"https://microsoft.com/link","expires_in":900,"interval":1,"message":"go"
        }))).mount(&s).await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token":"MSA","refresh_token":"MSR"})))
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/xbl"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Token":"XBL","DisplayClaims":{"xui":[{"uhs":"UHS"}]}})))
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/xsts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Token":"XSTS","DisplayClaims":{"xui":[{"uhs":"UHS"}]}})))
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/mclogin"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token":"MC","expires_in":86400})))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/ent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[{"name":"game_minecraft"}]})))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/profile"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"b50ad385829d3141a2167e7d7539ba7f","name":"Notch"})))
            .mount(&s)
            .await;

        let ep = eps(&s);
        let c = http();
        let dc = request_device_code(&c, &ep, "cid").await.unwrap();
        assert_eq!(dc.user_code, "ABCD");
        let t = poll_for_tokens(&c, &ep, "cid", &dc, || false).await.unwrap();
        assert_eq!(t.refresh_token, "MSR");
        let r = refresh(&c, &ep, "cid", "MSR").await.unwrap();
        assert_eq!(r.access_token, "MSA");
        let l = minecraft_from_ms(&c, &ep, &t.access_token).await.unwrap();
        assert_eq!(l.access_token, "MC");
        assert_eq!(l.profile.name, "Notch");
    }

    #[tokio::test]
    async fn not_owned_and_xsts_error() {
        let s = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/xsts"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({"XErr":2148916233u64})))
            .mount(&s)
            .await;
        let e = xsts(&http(), &eps(&s), "x").await.unwrap_err();
        assert!(e.to_string().contains("no Xbox account"));
        Mock::given(method("GET")).and(path("/profile")).respond_with(ResponseTemplate::new(404)).mount(&s).await;
        assert!(matches!(fetch_profile(&http(), &eps(&s), "t").await, Err(AccountError::NotOwned)));
    }
}
