//! Modrinth v2 API client. Response parsers are pure functions (fixture-tested).
use super::types::*;
use super::*;
use serde_json::json;

const BASE: &str = "https://api.modrinth.com/v2";

pub fn build_facets(kind: ResourceKind, q: &SearchQuery) -> String {
    let mut f: Vec<Value> = vec![json!([format!("project_type:{}", kind.modrinth_type())])];
    if let Some(l) = q.loader {
        let mut names = vec![format!("categories:{}", l.modrinth_name())];
        // Quilt can load Fabric mods
        if l == ModLoader::Quilt {
            names.push("categories:fabric".into());
        }
        f.push(json!(names));
    }
    if let Some(v) = &q.mc_version {
        f.push(json!([format!("versions:{v}")]));
    }
    for c in &q.categories {
        f.push(json!([format!("categories:{c}")]));
    }
    Value::Array(f).to_string()
}

pub fn parse_search(v: &Value) -> SearchResult {
    let hits = v["hits"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|h| {
                    let slug = s(h, "slug");
                    let ty = opt_s(h, "project_type").unwrap_or_else(|| "mod".into());
                    ProjectSummary {
                        platform: Platform::Modrinth,
                        id: opt_s(h, "project_id").or_else(|| opt_s(h, "id")).unwrap_or_default(),
                        title: s(h, "title"),
                        description: s(h, "description"),
                        icon_url: opt_s(h, "icon_url").filter(|x| !x.is_empty()),
                        downloads: u(h, "downloads"),
                        follows: u(h, "follows"),
                        categories: str_list(h, "display_categories"),
                        author: opt_s(h, "author"),
                        updated: opt_s(h, "date_modified"),
                        page_url: Some(format!("https://modrinth.com/{ty}/{slug}")),
                        slug,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    SearchResult { hits, total: u(v, "total_hits") }
}

pub fn parse_project(h: &Value) -> ProjectSummary {
    let slug = s(h, "slug");
    let ty = opt_s(h, "project_type").unwrap_or_else(|| "mod".into());
    ProjectSummary {
        platform: Platform::Modrinth,
        id: s(h, "id"),
        title: s(h, "title"),
        description: s(h, "description"),
        icon_url: opt_s(h, "icon_url").filter(|x| !x.is_empty()),
        downloads: u(h, "downloads"),
        follows: u(h, "followers"),
        categories: str_list(h, "categories"),
        author: None,
        updated: opt_s(h, "updated"),
        page_url: Some(format!("https://modrinth.com/{ty}/{slug}")),
        slug,
    }
}

pub fn parse_version(v: &Value) -> ResourceFile {
    let files = v["files"].as_array().cloned().unwrap_or_default();
    let primary = files
        .iter()
        .find(|f| f["primary"].as_bool().unwrap_or(false))
        .or_else(|| files.first())
        .cloned()
        .unwrap_or(Value::Null);
    let deps = v["dependencies"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|d| Dependency {
                    project_id: opt_s(d, "project_id"),
                    file_id: opt_s(d, "version_id"),
                    kind: match d["dependency_type"].as_str().unwrap_or("") {
                        "required" => DependencyKind::Required,
                        "optional" => DependencyKind::Optional,
                        "embedded" => DependencyKind::Embedded,
                        _ => DependencyKind::Incompatible,
                    },
                })
                .collect()
        })
        .unwrap_or_default();
    let mut loaders: Vec<ModLoader> = str_list(v, "loaders")
        .iter()
        .filter_map(|l| ModLoader::from_name(l))
        .collect();
    loaders.sort();
    loaders.dedup();
    ResourceFile {
        platform: Platform::Modrinth,
        id: s(v, "id"),
        project_id: s(v, "project_id"),
        name: s(v, "name"),
        file_name: s(&primary, "filename"),
        version_number: opt_s(v, "version_number"),
        release_type: s(v, "version_type"),
        game_versions: str_list(v, "game_versions"),
        loaders,
        sha1: primary["hashes"]["sha1"].as_str().map(str::to_string),
        size: u(&primary, "size"),
        download_urls: opt_s(&primary, "url").into_iter().collect(),
        date_published: s(v, "date_published"),
        dependencies: deps,
        fingerprint: None,
    }
}

pub async fn search(kind: ResourceKind, q: &SearchQuery) -> Result<SearchResult> {
    let size = if q.page_size == 0 { 20 } else { q.page_size.min(100) };
    let url = format!(
        "{BASE}/search?query={}&limit={size}&offset={}&index=relevance&facets={}",
        urlencode(&q.query),
        q.page * size,
        urlencode(&build_facets(kind, q))
    );
    Ok(parse_search(&send_json(http_client()?.get(url)).await?))
}

pub async fn project(id_or_slug: &str) -> Result<ProjectSummary> {
    let v = send_json(http_client()?.get(format!("{BASE}/project/{}", urlencode(id_or_slug)))).await?;
    Ok(parse_project(&v))
}

pub async fn projects(ids: &[String]) -> Result<Vec<ProjectSummary>> {
    let ids_json = serde_json::to_string(ids).map_err(|e| e.to_string())?;
    let v = send_json(http_client()?.get(format!("{BASE}/projects?ids={}", urlencode(&ids_json)))).await?;
    Ok(v.as_array().map(|a| a.iter().map(parse_project).collect()).unwrap_or_default())
}

pub async fn versions(
    project: &str,
    mc_version: Option<&str>,
    loader: Option<ModLoader>,
) -> Result<Vec<ResourceFile>> {
    let mut url = format!("{BASE}/project/{}/version?include_changelog=false", urlencode(project));
    if let Some(l) = loader {
        url += &format!("&loaders={}", urlencode(&json!([l.modrinth_name()]).to_string()));
    }
    if let Some(v) = mc_version {
        url += &format!("&game_versions={}", urlencode(&json!([v]).to_string()));
    }
    let v = send_json(http_client()?.get(url)).await?;
    Ok(v.as_array().map(|a| a.iter().map(parse_version).collect()).unwrap_or_default())
}

/// POST /version_files: map sha1 -> version.
pub async fn versions_by_hashes(hashes: &[String]) -> Result<std::collections::HashMap<String, ResourceFile>> {
    let body = json!({"hashes": hashes, "algorithm": "sha1"});
    let v = send_json(http_client()?.post(format!("{BASE}/version_files")).json(&body)).await?;
    Ok(parse_hash_map(&v))
}

pub fn parse_hash_map(v: &Value) -> std::collections::HashMap<String, ResourceFile> {
    v.as_object()
        .map(|o| o.iter().map(|(k, x)| (k.clone(), parse_version(x))).collect())
        .unwrap_or_default()
}

/// POST /version_files/update: latest version for each hash given loaders and MC versions.
pub async fn latest_by_hashes(
    hashes: &[String],
    loaders: &[ModLoader],
    mc_versions: &[String],
) -> Result<std::collections::HashMap<String, ResourceFile>> {
    let body = json!({
        "hashes": hashes, "algorithm": "sha1",
        "loaders": loaders.iter().map(|l| l.modrinth_name()).collect::<Vec<_>>(),
        "game_versions": mc_versions,
    });
    let v = send_json(http_client()?.post(format!("{BASE}/version_files/update")).json(&body)).await?;
    Ok(parse_hash_map(&v))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERSION: &str = r#"{"id":"V1","project_id":"P1","name":"Sodium 0.5","version_number":"0.5.0","version_type":"release",
      "game_versions":["1.20.1"],"loaders":["fabric","quilt","fabric"],"date_published":"2024-01-01T00:00:00Z",
      "dependencies":[{"project_id":"API","version_id":null,"dependency_type":"required"},{"project_id":"X","dependency_type":"incompatible"}],
      "files":[{"hashes":{"sha1":"aa"},"url":"https://cdn.modrinth.com/a.jar","filename":"a-src.jar","primary":false,"size":1},
               {"hashes":{"sha1":"bb"},"url":"https://cdn.modrinth.com/b.jar","filename":"b.jar","primary":true,"size":42}]}"#;

    #[test]
    fn version_parsing() {
        let f = parse_version(&serde_json::from_str(VERSION).unwrap());
        assert_eq!(f.file_name, "b.jar");
        assert_eq!(f.sha1.as_deref(), Some("bb"));
        assert_eq!(f.loaders, vec![ModLoader::Fabric, ModLoader::Quilt]);
        assert_eq!(f.dependencies[0].kind, DependencyKind::Required);
        assert_eq!(f.dependencies[1].kind, DependencyKind::Incompatible);
        assert_eq!(f.size, 42);
    }

    #[test]
    fn search_parsing() {
        let v: Value = serde_json::from_str(r#"{"hits":[{"project_id":"P","slug":"sodium","title":"Sodium","description":"d",
            "icon_url":"","downloads":5,"follows":2,"display_categories":["optimization"],"author":"jelly","project_type":"mod"}],"total_hits":9}"#).unwrap();
        let r = parse_search(&v);
        assert_eq!(r.total, 9);
        assert_eq!(r.hits[0].page_url.as_deref(), Some("https://modrinth.com/mod/sodium"));
        assert!(r.hits[0].icon_url.is_none());
    }

    #[test]
    fn facets() {
        let q = SearchQuery { mc_version: Some("1.20.1".into()), loader: Some(ModLoader::Fabric), ..Default::default() };
        assert_eq!(
            build_facets(ResourceKind::Mod, &q),
            r#"[["project_type:mod"],["categories:fabric"],["versions:1.20.1"]]"#
        );
    }

    #[test]
    fn hash_map() {
        let v: Value = serde_json::from_str(&format!(r#"{{"bb":{VERSION}}}"#)).unwrap();
        assert_eq!(parse_hash_map(&v)["bb"].project_id, "P1");
    }
}
