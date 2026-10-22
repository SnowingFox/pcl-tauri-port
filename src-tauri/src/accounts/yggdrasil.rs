//! Third-party Yggdrasil / authlib-injector servers.
use super::{dash, AccountError, Result};
use base64::Engine;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    /// Normalised API root, no trailing slash.
    pub url: String,
    pub name: String,
    /// Raw metadata JSON, base64, for `authlibinjector.yggdrasil.prefetched`.
    pub prefetched: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthResult {
    pub access_token: String,
    pub client_token: String,
    pub profile: Option<Profile>,
    pub available: Vec<Profile>,
}

/// Accepts `example.com/api`, `https://...`, or `authlib-injector:yggdrasil-server:<urlencoded>`.
pub fn normalize_url(input: &str) -> Result<String> {
    let mut s = input.trim().to_string();
    if let Some(rest) = s.strip_prefix("authlib-injector:yggdrasil-server:") {
        s = url::form_urlencoded::parse(format!("x={rest}").as_bytes())
            .next()
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default();
    }
    if !s.contains("://") {
        s = format!("https://{s}");
    }
    let u = url::Url::parse(&s).map_err(|e| AccountError::Invalid(format!("bad server URL: {e}")))?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err(AccountError::Invalid("server URL must be http(s)".into()));
    }
    Ok(u.as_str().trim_end_matches('/').to_string())
}

pub fn parse_meta_name(body: &str) -> Result<String> {
    let v: serde_json::Value = serde_json::from_str(body)?;
    Ok(v["meta"]["serverName"].as_str().unwrap_or("Third-party server").to_string())
}

/// Fetch metadata. Follows the `X-Authlib-Injector-API-Location` redirect hint once.
pub async fn discover(c: &reqwest::Client, input: &str) -> Result<ServerInfo> {
    let mut url = normalize_url(input)?;
    for _ in 0..2 {
        let r = c.get(&url).send().await?;
        if let Some(loc) = r.headers().get("x-authlib-injector-api-location").and_then(|v| v.to_str().ok()) {
            let next = r.url().join(loc).map_err(|e| AccountError::BadResponse(e.to_string()))?;
            let next = next.as_str().trim_end_matches('/').to_string();
            if next != url {
                url = next;
                continue;
            }
        }
        if !r.status().is_success() {
            return Err(AccountError::Server(format!("metadata request failed: HTTP {}", r.status())));
        }
        let body = r.text().await?;
        let name = parse_meta_name(&body)?;
        let prefetched = base64::engine::general_purpose::STANDARD.encode(body.as_bytes());
        return Ok(ServerInfo { url, name, prefetched });
    }
    Err(AccountError::Server("too many API location redirects".into()))
}

fn server_error(body: &str) -> AccountError {
    let msg = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["errorMessage"].as_str().map(String::from))
        .unwrap_or_else(|| body.chars().take(200).collect());
    AccountError::Server(msg)
}

pub fn parse_auth_response(body: &str) -> Result<AuthResult> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct R {
        access_token: String,
        client_token: String,
        selected_profile: Option<Profile>,
        #[serde(default)]
        available_profiles: Vec<Profile>,
    }
    let r: R = serde_json::from_str(body)?;
    Ok(AuthResult {
        access_token: r.access_token,
        client_token: r.client_token,
        profile: r.selected_profile,
        available: r.available_profiles,
    })
}

pub async fn authenticate(c: &reqwest::Client, server: &str, user: &str, password: &str, client_token: Option<&str>) -> Result<AuthResult> {
    let mut payload = json!({
        "agent": {"name": "Minecraft", "version": 1},
        "username": user, "password": password, "requestUser": false
    });
    if let Some(t) = client_token {
        payload["clientToken"] = t.into();
    }
    let r = c.post(format!("{server}/authserver/authenticate")).json(&payload).send().await?;
    let ok = r.status().is_success();
    let body = r.text().await?;
    if !ok {
        return Err(server_error(&body));
    }
    parse_auth_response(&body)
}

pub async fn refresh(c: &reqwest::Client, server: &str, access: &str, client: &str) -> Result<AuthResult> {
    let r = c
        .post(format!("{server}/authserver/refresh"))
        .json(&json!({"accessToken": access, "clientToken": client}))
        .send()
        .await?;
    let ok = r.status().is_success();
    let body = r.text().await?;
    if !ok {
        return Err(server_error(&body));
    }
    parse_auth_response(&body)
}

/// 204 = valid, 403 = invalid.
pub async fn validate(c: &reqwest::Client, server: &str, access: &str, client: &str) -> Result<bool> {
    let r = c
        .post(format!("{server}/authserver/validate"))
        .json(&json!({"accessToken": access, "clientToken": client}))
        .send()
        .await?;
    match r.status().as_u16() {
        204 | 200 => Ok(true),
        403 => Ok(false),
        s => Err(AccountError::Server(format!("validate failed: HTTP {s}"))),
    }
}

#[allow(dead_code)]
pub async fn invalidate(c: &reqwest::Client, server: &str, access: &str, client: &str) -> Result<()> {
    c.post(format!("{server}/authserver/invalidate"))
        .json(&json!({"accessToken": access, "clientToken": client}))
        .send()
        .await?;
    Ok(())
}

pub fn dashed(p: &Profile) -> String {
    dash(&p.id)
}

/// Where the launcher should store/download authlib-injector (hook for the download module).
pub const AUTHLIB_INJECTOR_LATEST_META: &str = "https://authlib-injector.yushi.moe/artifact/latest.json";

/// Resolve the latest authlib-injector artifact `(download_url, sha256)`.
/// The launch-core / download agent performs the actual download to
/// `<launcher dir>/authlib-injector.jar` and substitutes the path in `-javaagent`.
pub async fn latest_injector(c: &reqwest::Client, meta_url: &str) -> Result<(String, Option<String>)> {
    let v: serde_json::Value = c.get(meta_url).send().await?.error_for_status()?.json().await?;
    let url = v["download_url"].as_str().ok_or_else(|| AccountError::BadResponse("no download_url".into()))?;
    let sha = v["checksums"]["sha256"].as_str().map(String::from);
    Ok((url.into(), sha))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn urls() {
        assert_eq!(normalize_url("example.com/api/yggdrasil/").unwrap(), "https://example.com/api/yggdrasil");
        assert_eq!(
            normalize_url("authlib-injector:yggdrasil-server:https%3A%2F%2Fa.b%2Fapi").unwrap(),
            "https://a.b/api"
        );
        assert!(normalize_url("ftp://x").is_err());
    }

    #[test]
    fn auth_parse() {
        let r = parse_auth_response(r#"{"accessToken":"A","clientToken":"C","selectedProfile":{"id":"b50ad385829d3141a2167e7d7539ba7f","name":"N"},"availableProfiles":[{"id":"1","name":"N"}]}"#).unwrap();
        assert_eq!(r.access_token, "A");
        assert_eq!(dashed(&r.profile.unwrap()), "b50ad385-829d-3141-a216-7e7d7539ba7f");
        let r = parse_auth_response(r#"{"accessToken":"A","clientToken":"C","availableProfiles":[{"id":"1","name":"N"},{"id":"2","name":"M"}]}"#).unwrap();
        assert!(r.profile.is_none());
        assert_eq!(r.available.len(), 2);
    }

    #[tokio::test]
    async fn mocked_server() {
        let s = MockServer::start().await;
        Mock::given(method("GET")).and(path("/")).respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"serverName":"Test"}}"#)).mount(&s).await;
        Mock::given(method("POST")).and(path("/authserver/authenticate")).respond_with(ResponseTemplate::new(200).set_body_string(r#"{"accessToken":"A","clientToken":"C","selectedProfile":{"id":"abc","name":"N"}}"#)).mount(&s).await;
        Mock::given(method("POST")).and(path("/authserver/validate")).respond_with(ResponseTemplate::new(403)).mount(&s).await;
        let c = reqwest::Client::new();
        let info = discover(&c, &s.uri()).await.unwrap();
        assert_eq!(info.name, "Test");
        assert!(!info.prefetched.is_empty());
        let a = authenticate(&c, &info.url, "u", "p", None).await.unwrap();
        assert_eq!(a.client_token, "C");
        assert!(!validate(&c, &info.url, "A", "C").await.unwrap());
    }

    #[tokio::test]
    async fn auth_failure_message() {
        let s = MockServer::start().await;
        Mock::given(method("POST")).and(path("/authserver/authenticate")).respond_with(ResponseTemplate::new(403).set_body_string(r#"{"error":"ForbiddenOperationException","errorMessage":"Invalid credentials."}"#)).mount(&s).await;
        let e = authenticate(&reqwest::Client::new(), &s.uri(), "u", "p", None).await.unwrap_err();
        assert!(e.to_string().contains("Invalid credentials"));
    }
}
