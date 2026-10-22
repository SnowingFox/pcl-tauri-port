//! Skin / avatar helpers. The frontend crops the head from the returned skin PNG
//! (canvas, 8x8 at (8,8) plus hat layer at (40,8)); no image crate needed here.
use super::{AccountError, Result, microsoft::Endpoints};
use base64::Engine;

pub const MOJANG_SESSION_PROFILE: &str = "https://sessionserver.mojang.com/session/minecraft/profile";

/// Default skin for UUIDs without a custom one: true = Alex (slim), false = Steve.
/// Mirrors `UUID.hashCode() & 1` from Java.
pub fn default_is_alex(uuid: &str) -> bool {
    let u = super::undash(uuid);
    if u.len() != 32 {
        return false;
    }
    let hi = u64::from_str_radix(&u[..16], 16).unwrap_or(0);
    let lo = u64::from_str_radix(&u[16..], 16).unwrap_or(0);
    let x = hi ^ lo;
    (((x >> 32) as u32) ^ (x as u32)) & 1 == 1
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinInfo {
    pub url: Option<String>,
    pub slim: bool,
}

/// Parse a session-server profile JSON (Mojang or Yggdrasil) and extract the skin.
pub fn parse_textures(body: &str) -> Result<SkinInfo> {
    let v: serde_json::Value = serde_json::from_str(body)?;
    let prop = v["properties"].as_array().and_then(|a| a.iter().find(|p| p["name"] == "textures")).and_then(|p| p["value"].as_str());
    let Some(b64) = prop else { return Ok(SkinInfo { url: None, slim: false }) };
    let raw = base64::engine::general_purpose::STANDARD.decode(b64).map_err(|e| AccountError::BadResponse(e.to_string()))?;
    let t: serde_json::Value = serde_json::from_slice(&raw)?;
    let skin = &t["textures"]["SKIN"];
    Ok(SkinInfo { url: skin["url"].as_str().map(String::from), slim: skin["metadata"]["model"] == "slim" })
}

/// `profile_base` is the session-server profile URL prefix
/// (Mojang default or `<yggdrasil>/sessionserver/session/minecraft/profile`).
pub async fn fetch_skin_info(c: &reqwest::Client, profile_base: &str, uuid: &str) -> Result<SkinInfo> {
    let r = c.get(format!("{profile_base}/{}", super::undash(uuid))).send().await?;
    if r.status().as_u16() == 204 || r.status().as_u16() == 404 {
        return Ok(SkinInfo { url: None, slim: default_is_alex(uuid) });
    }
    parse_textures(&r.error_for_status()?.text().await?)
}

/// Fetch the skin PNG as a `data:` URL (https only). Falls back to None.
pub async fn skin_data_url(c: &reqwest::Client, url: &str) -> Result<String> {
    // The URL comes from a (possibly third-party) session server response.
    let allow = crate::settings::guard::HostPolicy::AllowLan;
    crate::settings::guard::check_remote_url(url, allow).map_err(|e| AccountError::Invalid(format!("skin URL: {e}")))?;
    let bytes = c.get(url).send().await?.error_for_status()?.bytes().await?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[allow(dead_code)]
pub fn mojang_endpoints() -> Endpoints {
    Endpoints::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alex_steve() {
        // Known: Notch's offline UUID hashes to even (Steve); just assert determinism + both outcomes exist.
        assert_eq!(default_is_alex("b50ad385-829d-3141-a216-7e7d7539ba7f"), default_is_alex("b50ad385829d3141a2167e7d7539ba7f"));
        assert!(default_is_alex("00000000-0000-0000-0000-000000000001"));
        assert!(!default_is_alex("00000000-0000-0000-0000-000000000000"));
    }

    #[tokio::test]
    async fn skin_urls_are_checked_before_any_request() {
        let c = reqwest::Client::new();
        for bad in ["file:///etc/passwd", "http://127.0.0.1:1/skin.png", "http://169.254.169.254/latest/meta-data", "gopher://x/y"] {
            let e = skin_data_url(&c, bad).await.unwrap_err();
            assert!(matches!(e, AccountError::Invalid(_)), "{bad}: {e}");
        }
    }

    #[test]
    fn textures() {
        let inner = r#"{"textures":{"SKIN":{"url":"https://t/x.png","metadata":{"model":"slim"}}}}"#;
        let b = base64::engine::general_purpose::STANDARD.encode(inner);
        let body = format!(r#"{{"id":"1","name":"n","properties":[{{"name":"textures","value":"{b}"}}]}}"#);
        let s = parse_textures(&body).unwrap();
        assert_eq!(s.url.as_deref(), Some("https://t/x.png"));
        assert!(s.slim);
        assert_eq!(parse_textures(r#"{"properties":[]}"#).unwrap().url, None);
    }
}
