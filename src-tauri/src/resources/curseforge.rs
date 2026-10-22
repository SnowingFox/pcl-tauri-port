//! CurseForge v1 API client. The API key comes from `PCL_CURSEFORGE_KEY` (never embedded).
use super::types::*;
use super::*;
use serde_json::json;

const BASE: &str = "https://api.curseforge.com/v1";
const GAME_ID: u32 = 432;
pub const KEY_ENV: &str = "PCL_CURSEFORGE_KEY";

fn key() -> Result<String> {
    std::env::var(KEY_ENV)
        .ok()
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| format!("CurseForge API key not configured: set the {KEY_ENV} environment variable"))
}

fn client_req(method: reqwest::Method, path: &str) -> Result<reqwest::RequestBuilder> {
    Ok(http_client()?.request(method, format!("{BASE}{path}")).header("x-api-key", key()?))
}

pub fn parse_mod(m: &Value) -> ProjectSummary {
    ProjectSummary {
        platform: Platform::CurseForge,
        id: m["id"].as_u64().map(|i| i.to_string()).unwrap_or_default(),
        slug: s(m, "slug"),
        title: s(m, "name"),
        description: s(m, "summary"),
        icon_url: m["logo"]["thumbnailUrl"].as_str().filter(|x| !x.is_empty()).map(str::to_string),
        downloads: m["downloadCount"].as_f64().unwrap_or(0.0) as u64,
        follows: u(m, "thumbsUpCount"),
        categories: m["categories"]
            .as_array()
            .map(|a| a.iter().filter_map(|c| c["name"].as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        author: m["authors"][0]["name"].as_str().map(str::to_string),
        updated: opt_s(m, "dateModified"),
        page_url: m["links"]["websiteUrl"].as_str().map(str::to_string),
    }
}

pub fn parse_search(v: &Value) -> SearchResult {
    SearchResult {
        hits: v["data"].as_array().map(|a| a.iter().map(parse_mod).collect()).unwrap_or_default(),
        total: v["pagination"]["totalCount"].as_u64().unwrap_or(0),
    }
}

/// Game versions list mixes MC versions with loader names ("Fabric") and env tags ("Client").
pub fn parse_file(f: &Value) -> ResourceFile {
    let gv = str_list(f, "gameVersions");
    let mut loaders: Vec<ModLoader> = gv.iter().filter_map(|x| ModLoader::from_name(x)).collect();
    loaders.sort();
    loaders.dedup();
    let game_versions = gv
        .into_iter()
        .filter(|x| x.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .collect();
    let sha1 = f["hashes"]
        .as_array()
        .and_then(|h| h.iter().find(|x| x["algo"].as_u64() == Some(1)))
        .and_then(|x| x["value"].as_str())
        .map(str::to_string);
    let rt = match f["releaseType"].as_u64() {
        Some(1) => "release",
        Some(2) => "beta",
        _ => "alpha",
    };
    let deps = f["dependencies"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|d| Dependency {
                    project_id: d["modId"].as_u64().map(|i| i.to_string()),
                    file_id: None,
                    kind: match d["relationType"].as_u64() {
                        Some(3) => DependencyKind::Required,
                        Some(2) => DependencyKind::Optional,
                        Some(1) => DependencyKind::Embedded,
                        Some(4) => DependencyKind::Tool,
                        _ => DependencyKind::Incompatible,
                    },
                })
                .collect()
        })
        .unwrap_or_default();
    let file_name = s(f, "fileName");
    ResourceFile {
        platform: Platform::CurseForge,
        id: f["id"].as_u64().map(|i| i.to_string()).unwrap_or_default(),
        project_id: f["modId"].as_u64().map(|i| i.to_string()).unwrap_or_default(),
        name: opt_s(f, "displayName").unwrap_or_else(|| file_name.clone()),
        file_name,
        version_number: None,
        release_type: rt.into(),
        game_versions,
        loaders,
        sha1,
        size: u(f, "fileLength"),
        download_urls: opt_s(f, "downloadUrl").filter(|x| !x.is_empty()).into_iter().collect(),
        date_published: s(f, "fileDate"),
        dependencies: deps,
        fingerprint: f["fileFingerprint"].as_u64().map(|x| x as u32),
    }
}

/// Files whose author disabled API distribution have `downloadUrl: null`. The CDN path is
/// deterministic: https://edge.forgecdn.net/files/{id/1000}/{id%1000}/{fileName}.
pub fn fallback_download_url(file_id: u64, file_name: &str) -> String {
    format!("https://edge.forgecdn.net/files/{}/{}/{}", file_id / 1000, file_id % 1000, urlencode(file_name))
}

pub async fn search(kind: ResourceKind, q: &SearchQuery) -> Result<SearchResult> {
    let size = if q.page_size == 0 { 20 } else { q.page_size.min(50) };
    let mut path = format!(
        "/mods/search?gameId={GAME_ID}&classId={}&sortField=2&sortOrder=desc&pageSize={size}&index={}",
        kind.curseforge_class(),
        q.page * size
    );
    if !q.query.is_empty() {
        path += &format!("&searchFilter={}", urlencode(&q.query));
    }
    if let Some(l) = q.loader {
        path += &format!("&modLoaderType={}", l.curseforge_id());
    }
    if let Some(v) = &q.mc_version {
        path += &format!("&gameVersion={}", urlencode(v));
    }
    Ok(parse_search(&send_json(client_req(reqwest::Method::GET, &path)?).await?))
}

pub async fn project(id: &str) -> Result<ProjectSummary> {
    let v = send_json(client_req(reqwest::Method::GET, &format!("/mods/{}", urlencode(id)))?).await?;
    Ok(parse_mod(&v["data"]))
}

pub async fn files(
    mod_id: &str,
    mc_version: Option<&str>,
    loader: Option<ModLoader>,
) -> Result<Vec<ResourceFile>> {
    let mut path = format!("/mods/{}/files?pageSize=50", urlencode(mod_id));
    if let Some(v) = mc_version {
        path += &format!("&gameVersion={}", urlencode(v));
    }
    if let Some(l) = loader {
        path += &format!("&modLoaderType={}", l.curseforge_id());
    }
    let v = send_json(client_req(reqwest::Method::GET, &path)?).await?;
    Ok(v["data"].as_array().map(|a| a.iter().map(parse_file).collect()).unwrap_or_default())
}

/// POST /mods/files. Deleted files are silently omitted by the API.
pub async fn files_by_ids(ids: &[u64]) -> Result<Vec<ResourceFile>> {
    let v = send_json(client_req(reqwest::Method::POST, "/mods/files")?.json(&json!({"fileIds": ids}))).await?;
    Ok(v["data"].as_array().map(|a| a.iter().map(parse_file).collect()).unwrap_or_default())
}

/// Resolve a download URL: field from the file listing, else GET download-url, else CDN fallback.
pub async fn resolve_download_url(mod_id: &str, file_id: u64, file_name: &str) -> Result<String> {
    let path = format!("/mods/{}/files/{file_id}/download-url", urlencode(mod_id));
    if let Ok(v) = send_json(client_req(reqwest::Method::GET, &path)?).await {
        if let Some(u) = v["data"].as_str().filter(|x| !x.is_empty()) {
            return Ok(u.to_string());
        }
    }
    Ok(fallback_download_url(file_id, file_name))
}

/// POST /fingerprints/432: fingerprint -> matching file.
pub async fn match_fingerprints(fps: &[u32]) -> Result<Vec<(u32, ResourceFile)>> {
    let v = send_json(client_req(reqwest::Method::POST, &format!("/fingerprints/{GAME_ID}"))?.json(&json!({"fingerprints": fps}))).await?;
    Ok(parse_fingerprint_matches(&v))
}

pub fn parse_fingerprint_matches(v: &Value) -> Vec<(u32, ResourceFile)> {
    v["data"]["exactMatches"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    let f = parse_file(&m["file"]);
                    f.fingerprint.map(|fp| (fp, f))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_parsing() {
        let v: Value = serde_json::from_str(r#"{"id":4001,"modId":238222,"displayName":"JEI 15","fileName":"jei-1.20.1.jar",
          "releaseType":1,"gameVersions":["1.20.1","Forge","Client"],"downloadUrl":null,"fileLength":10,"fileDate":"2024-02-02T00:00:00Z",
          "hashes":[{"value":"abc","algo":1},{"value":"d","algo":2}],"fileFingerprint":123456,
          "dependencies":[{"modId":9,"relationType":3}]}"#).unwrap();
        let f = parse_file(&v);
        assert_eq!(f.loaders, vec![ModLoader::Forge]);
        assert_eq!(f.game_versions, vec!["1.20.1"]);
        assert_eq!(f.sha1.as_deref(), Some("abc"));
        assert!(f.download_urls.is_empty());
        assert_eq!(f.fingerprint, Some(123456));
        assert_eq!(f.dependencies[0].kind, DependencyKind::Required);
        assert_eq!(fallback_download_url(4001, "jei-1.20.1.jar"), "https://edge.forgecdn.net/files/4/1/jei-1.20.1.jar");
    }

    #[test]
    fn search_parsing() {
        let v: Value = serde_json::from_str(r#"{"data":[{"id":1,"slug":"jei","name":"JEI","summary":"s","downloadCount":1e6,
          "logo":{"thumbnailUrl":"https://x/y.png"},"categories":[{"name":"Utility"}],"authors":[{"name":"mezz"}],
          "links":{"websiteUrl":"https://www.curseforge.com/minecraft/mc-mods/jei"}}],"pagination":{"totalCount":3}}"#).unwrap();
        let r = parse_search(&v);
        assert_eq!(r.total, 3);
        assert_eq!(r.hits[0].downloads, 1_000_000);
        assert_eq!(r.hits[0].author.as_deref(), Some("mezz"));
    }

    #[test]
    fn fingerprint_matches() {
        let v: Value = serde_json::from_str(r#"{"data":{"exactMatches":[{"id":1,"file":{"id":2,"modId":1,"fileName":"a.jar","fileFingerprint":77,"gameVersions":[]}}]}}"#).unwrap();
        assert_eq!(parse_fingerprint_matches(&v)[0].0, 77);
    }
}
