// Mods, resource packs and shader packs from Modrinth and CurseForge, installed straight into an instance along with
// the mods they require. Modrinth needs no key; CurseForge needs the player's own API key (Settings).
use crate::mc::{create_instance, game_dir, instance_dir, load_instance, progress, safe_rel, zip_entry, Instance};
use crate::net::{self, client, Job};
use crate::store::{e, read_json, root, text, write_json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

pub(crate) const MODRINTH: &str = "https://api.modrinth.com/v2";
const CURSEFORGE: &str = "https://api.curseforge.com/v1";

/// Fabric API's project ids on Modrinth (id and slug) and CurseForge.
pub(crate) const FABRIC_API: [&str; 3] = ["P7dR8mSH", "fabric-api", "306612"];

pub(crate) fn folder(kind: &str) -> Result<&'static str, String> {
    match kind {
        "mod" => Ok("mods"),
        "resourcepack" => Ok("resourcepacks"),
        "shader" => Ok("shaderpacks"),
        _ => Err("Unknown kind of content.".into()),
    }
}

fn cf_class(kind: &str) -> u32 {
    match kind {
        "resourcepack" => 12,
        "shader" => 6552,
        "modpack" => 4471,
        _ => 6,
    }
}

fn cf_loader(loader: &str) -> Option<u32> {
    match loader {
        "forge" => Some(1),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" => Some(6),
        _ => None,
    }
}

/// Loaders whose mods run on this one: Quilt runs Fabric mods, NeoForge for 1.20.1 runs Forge mods.
pub(crate) fn mr_loaders(loader: &str, mc: &str) -> Vec<&'static str> {
    match loader {
        "fabric" => vec!["fabric"],
        "quilt" => vec!["quilt", "fabric"],
        "forge" => vec!["forge"],
        "neoforge" if mc == "1.20.1" => vec!["neoforge", "forge"],
        "neoforge" => vec!["neoforge"],
        _ => Vec::new(),
    }
}

fn valid_project(p: &str) -> Result<&str, String> {
    if !p.is_empty() && p.len() <= 64 && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        Ok(p)
    } else {
        Err("That project id isn't valid.".into())
    }
}

/// A file name straight from a mod site, refused if it could reach outside the instance's folder.
pub(crate) fn safe_file(name: &str) -> Result<String, String> {
    let ok = !name.is_empty() && name.len() <= 200 && !name.starts_with('.') && !name.contains(['/', '\\', ':', '"', '<', '>', '|', '?', '*']);
    if ok {
        Ok(name.to_string())
    } else {
        Err(format!("Refused the file name {name}."))
    }
}

/// The launcher's own CurseForge API key. It is built in when a release is made (release.ps1 reads it from outside
/// the repo), so it never sits in the code; a build made without it simply has no CurseForge.
const CURSEFORGE_KEY: Option<&str> = option_env!("AH_CURSEFORGE_KEY");

fn cf_key() -> Result<String, String> {
    match CURSEFORGE_KEY.map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => Ok(k.to_string()),
        None => Err("CurseForge isn't available in this version of the launcher yet.".into()),
    }
}

#[derive(Serialize)]
pub struct Features {
    curseforge: bool,
}

/// What this build of the launcher can do, for the window to show only what works.
#[tauri::command]
pub fn features() -> Features {
    Features { curseforge: cf_key().is_ok() }
}

async fn cf_get(url: &str, key: &str, params: &[(&str, String)]) -> Result<Value, String> {
    let resp = client()
        .get(url)
        .header("x-api-key", key)
        .header("Accept", "application/json")
        .query(params)
        .send()
        .await
        .map_err(|err| format!("Couldn't reach CurseForge ({err})"))?;
    match resp.status().as_u16() {
        200 => resp.json().await.map_err(e),
        401 | 403 => Err("CurseForge turned the launcher's key away. Try again later.".into()),
        404 => Err("CurseForge doesn't have that project.".into()),
        code => Err(format!("CurseForge answered {code}.")),
    }
}

pub(crate) async fn mr_get(url: &str, params: &[(&str, String)]) -> Result<Value, String> {
    let resp = client().get(url).query(params).send().await.map_err(|err| format!("Couldn't reach Modrinth ({err})"))?;
    match resp.status().as_u16() {
        200 => resp.json().await.map_err(e),
        404 => Err("Modrinth doesn't have that project.".into()),
        429 => Err("Modrinth asked us to slow down. Try again in a minute.".into()),
        code => Err(format!("Modrinth answered {code}.")),
    }
}

#[derive(Serialize)]
pub struct Hit {
    source: String,
    id: String,
    title: String,
    author: String,
    description: String,
    downloads: u64,
    icon: String,
    url: String,
    /// What it is about (Optimization, Utility...), without the loader names Modrinth mixes in.
    categories: Vec<String>,
    /// When it last changed, as an ISO date.
    updated: String,
    follows: u64,
}

/// Modrinth mixes loaders and resolutions into a project's categories; only these are what it is about.
const CATEGORIES: [&str; 52] = [
    "adventure", "cursed", "decoration", "economy", "equipment", "food", "game-mechanics", "library", "magic", "management",
    "minigame", "mobs", "optimization", "social", "storage", "technology", "transportation", "utility", "worldgen",
    "audio", "blocks", "combat", "core-shaders", "entities", "environment", "fonts", "gui", "items", "locale", "models",
    "realistic", "simplistic", "themed", "tweaks", "vanilla-like", "atmosphere", "bloom", "cartoon", "colored-lighting",
    "fantasy", "foliage", "path-tracing", "pbr", "potato", "reflections", "screenshot", "semi-realistic", "shadows",
    "challenging", "kitchen-sink", "lightweight", "quests",
];

/// The ways a search can be ordered, by Modrinth's names.
fn valid_sort(sort: &str, query: &str) -> &'static str {
    match sort {
        "downloads" => "downloads",
        "follows" => "follows",
        "newest" => "newest",
        "updated" => "updated",
        "relevance" => "relevance",
        // No choice made: by relevance when searching for something, otherwise most downloaded first.
        _ if query.is_empty() => "downloads",
        _ => "relevance",
    }
}

#[derive(Serialize)]
pub struct Page {
    hits: Vec<Hit>,
    total: u64,
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn search_mods(
    source: String,
    query: String,
    kind: String,
    mc_version: String,
    loader: String,
    offset: u32,
    sort: Option<String>,
    category: Option<String>,
) -> Result<Page, String> {
    if kind != "modpack" {
        folder(&kind)?;
    }
    let q: String = query.trim().chars().take(100).collect();
    let sort = valid_sort(sort.as_deref().unwrap_or(""), &q);
    // A category is a short Modrinth slug like "optimization" or "game-mechanics".
    let category = category.filter(|c| !c.is_empty() && c.len() <= 32 && c.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-'));
    if source == "curseforge" {
        cf_search(&q, &kind, &mc_version, &loader, offset, sort).await
    } else {
        mr_search(&q, &kind, &mc_version, &loader, offset, sort, category.as_deref()).await
    }
}

async fn mr_search(q: &str, kind: &str, mc: &str, loader: &str, offset: u32, sort: &str, category: Option<&str>) -> Result<Page, String> {
    let mut facets = vec![vec![format!("project_type:{kind}")]];
    // A modpack brings its own Minecraft version, so packs for every version are shown.
    if !mc.is_empty() && kind != "modpack" {
        facets.push(vec![format!("versions:{mc}")]);
    }
    let loaders = mr_loaders(loader, mc);
    if kind == "mod" && !loaders.is_empty() {
        facets.push(loaders.iter().map(|l| format!("categories:{l}")).collect());
    }
    if let Some(c) = category {
        facets.push(vec![format!("categories:{c}")]);
    }
    let params = [
        ("query", q.to_string()),
        ("facets", serde_json::to_string(&facets).map_err(e)?),
        ("limit", "24".to_string()),
        ("offset", offset.to_string()),
        ("index", sort.to_string()),
    ];
    let v = mr_get(&format!("{MODRINTH}/search"), &params).await?;
    let hits = v
        .get("hits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|h| Hit {
            source: "modrinth".into(),
            id: text(h, "project_id"),
            title: text(h, "title"),
            author: text(h, "author"),
            description: text(h, "description"),
            downloads: h.get("downloads").and_then(Value::as_u64).unwrap_or(0),
            icon: text(h, "icon_url"),
            url: format!("https://modrinth.com/{}/{}", text(h, "project_type"), text(h, "slug")),
            categories: h
                .get("display_categories")
                .or(h.get("categories"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|c| CATEGORIES.contains(c))
                .take(3)
                .map(String::from)
                .collect(),
            updated: text(h, "date_modified"),
            follows: h.get("follows").and_then(Value::as_u64).unwrap_or(0),
        })
        .collect();
    Ok(Page { hits, total: v.get("total_hits").and_then(Value::as_u64).unwrap_or(0) })
}

async fn cf_search(q: &str, kind: &str, mc: &str, loader: &str, offset: u32, sort: &str) -> Result<Page, String> {
    let key = cf_key()?;
    // CurseForge's sort fields: 2 popularity, 3 last updated, 6 total downloads, 11 release date, 12 rating.
    let field = match sort {
        "downloads" => "6",
        "updated" => "3",
        "newest" => "11",
        "follows" => "12",
        _ => "2",
    };
    let mut params: Vec<(&str, String)> = vec![
        ("gameId", "432".into()),
        ("classId", cf_class(kind).to_string()),
        ("searchFilter", q.into()),
        ("sortField", field.into()),
        ("sortOrder", "desc".into()),
        ("pageSize", "24".into()),
        // CurseForge stops paging at 10,000 results.
        ("index", offset.min(9_975).to_string()),
    ];
    if !mc.is_empty() && kind != "modpack" {
        params.push(("gameVersion", mc.into()));
    }
    if kind == "mod" {
        if let Some(t) = cf_loader(loader) {
            params.push(("modLoaderType", t.to_string()));
        }
    }
    let v = cf_get(&format!("{CURSEFORGE}/mods/search"), &key, &params).await?;
    let hits = v
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|m| Hit {
            source: "curseforge".into(),
            id: m.get("id").map(|x| x.to_string()).unwrap_or_default(),
            title: text(m, "name"),
            author: m.pointer("/authors/0/name").and_then(Value::as_str).unwrap_or_default().to_string(),
            description: text(m, "summary"),
            downloads: m.get("downloadCount").and_then(Value::as_f64).unwrap_or(0.0) as u64,
            icon: m.pointer("/logo/thumbnailUrl").and_then(Value::as_str).unwrap_or_default().to_string(),
            url: m.pointer("/links/websiteUrl").and_then(Value::as_str).unwrap_or_default().to_string(),
            categories: m
                .get("categories")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|c| c.get("name").and_then(Value::as_str))
                .take(3)
                .map(String::from)
                .collect(),
            updated: text(m, "dateModified"),
            follows: m.get("thumbsUpCount").and_then(Value::as_u64).unwrap_or(0),
        })
        .collect();
    let total = v.pointer("/pagination/totalCount").and_then(Value::as_u64).unwrap_or(0).min(10_000);
    Ok(Page { hits, total })
}

// ---------------------------------------------------------------- one project's page

#[derive(Serialize)]
pub struct Link {
    label: String,
    url: String,
}

#[derive(Serialize)]
pub struct Shot {
    url: String,
    /// The full-size image, for viewing one large.
    full: String,
    title: String,
}

#[derive(Serialize)]
pub struct Details {
    source: String,
    id: String,
    title: String,
    summary: String,
    /// The long description: Markdown (Modrinth, often with HTML in it) or HTML (CurseForge).
    body: String,
    markdown: bool,
    icon: String,
    authors: String,
    gallery: Vec<Shot>,
    categories: Vec<String>,
    downloads: u64,
    follows: u64,
    license: String,
    updated: String,
    links: Vec<Link>,
    url: String,
    /// Where it has to be installed: "required", "optional" or "unsupported", for the client and the server.
    client: String,
    server: String,
}

fn https(url: &str) -> Option<String> {
    url.starts_with("https://").then(|| url.to_string())
}

#[tauri::command]
pub async fn project_details(source: String, id: String) -> Result<Details, String> {
    let id = valid_project(&id)?.to_string();
    if source == "curseforge" {
        let key = cf_key()?;
        let (mod_url, desc_url) = (format!("{CURSEFORGE}/mods/{id}"), format!("{CURSEFORGE}/mods/{id}/description"));
        let (m, desc) = futures::try_join!(cf_get(&mod_url, &key, &[]), cf_get(&desc_url, &key, &[]))?;
        let m = m.get("data").cloned().unwrap_or(Value::Null);
        let link = |label: &str, ptr: &str| m.pointer(ptr).and_then(Value::as_str).and_then(https).map(|url| Link { label: label.into(), url });
        return Ok(Details {
            source,
            id,
            title: text(&m, "name"),
            summary: text(&m, "summary"),
            body: text(&desc, "data"),
            markdown: false,
            icon: m.pointer("/logo/thumbnailUrl").and_then(Value::as_str).unwrap_or_default().to_string(),
            authors: m.get("authors").and_then(Value::as_array).into_iter().flatten().filter_map(|a| a.get("name").and_then(Value::as_str)).take(3).collect::<Vec<_>>().join(", "),
            gallery: m
                .get("screenshots")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|x| {
                    let url = https(x.get("thumbnailUrl").and_then(Value::as_str).unwrap_or(x.get("url")?.as_str()?))?;
                    Some(Shot { full: https(x.get("url")?.as_str()?)?, url, title: text(x, "title") })
                })
                .take(12)
                .collect(),
            categories: m.get("categories").and_then(Value::as_array).into_iter().flatten().filter_map(|c| c.get("name").and_then(Value::as_str)).take(4).map(String::from).collect(),
            downloads: m.get("downloadCount").and_then(Value::as_f64).unwrap_or(0.0) as u64,
            follows: m.get("thumbsUpCount").and_then(Value::as_u64).unwrap_or(0),
            license: String::new(),
            updated: text(&m, "dateModified"),
            links: [link("Source code", "/links/sourceUrl"), link("Issues", "/links/issuesUrl"), link("Wiki", "/links/wikiUrl")].into_iter().flatten().collect(),
            url: m.pointer("/links/websiteUrl").and_then(Value::as_str).unwrap_or_default().to_string(),
            client: String::new(),
            server: String::new(),
        });
    }
    let (project_url, members_url) = (format!("{MODRINTH}/project/{id}"), format!("{MODRINTH}/project/{id}/members"));
    let (p, members) = futures::try_join!(mr_get(&project_url, &[]), async { Ok::<Value, String>(mr_get(&members_url, &[]).await.unwrap_or(Value::Null)) })?;
    let mut gallery: Vec<&Value> = p.get("gallery").and_then(Value::as_array).into_iter().flatten().collect();
    // Featured shots first, then in the author's order.
    gallery.sort_by_key(|g| (g.get("featured").and_then(Value::as_bool) != Some(true), g.get("ordering").and_then(Value::as_i64).unwrap_or(0)));
    let link = |label: &str, key: &str| https(&text(&p, key)).map(|url| Link { label: label.into(), url });
    let owners: Vec<String> = members
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m.pointer("/user/username").and_then(Value::as_str).map(String::from))
        .take(3)
        .collect();
    let license = p.pointer("/license/name").and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from).unwrap_or_else(|| p.pointer("/license/id").and_then(Value::as_str).unwrap_or_default().to_string());
    Ok(Details {
        source,
        title: text(&p, "title"),
        summary: text(&p, "description"),
        body: text(&p, "body"),
        markdown: true,
        icon: text(&p, "icon_url"),
        authors: owners.join(", "),
        gallery: gallery
            .into_iter()
            .filter_map(|g| {
                let url = https(g.get("url")?.as_str()?)?;
                Some(Shot { full: g.get("raw_url").and_then(Value::as_str).and_then(https).unwrap_or_else(|| url.clone()), url, title: text(g, "title") })
            })
            .take(12)
            .collect(),
        categories: p.get("categories").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).filter(|c| CATEGORIES.contains(c)).map(String::from).collect(),
        downloads: p.get("downloads").and_then(Value::as_u64).unwrap_or(0),
        follows: p.get("followers").and_then(Value::as_u64).unwrap_or(0),
        // "LicenseRef-Polyform-Shield-1.0.0" reads better as "Polyform Shield 1.0.0".
        license: match license.strip_prefix("LicenseRef-") {
            Some("All-Rights-Reserved") => "All rights reserved".into(),
            Some(rest) => rest.replace('-', " "),
            None => license,
        },
        updated: text(&p, "updated"),
        links: [link("Source code", "source_url"), link("Issues", "issues_url"), link("Wiki", "wiki_url"), link("Discord", "discord_url")].into_iter().flatten().collect(),
        url: format!("https://modrinth.com/{}/{}", text(&p, "project_type"), text(&p, "slug")),
        client: text(&p, "client_side"),
        server: text(&p, "server_side"),
        id,
    })
}

// ---------------------------------------------------------------- installing

/// What the launcher remembers about each file it installed, keyed by file name.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub(crate) struct Entry {
    pub source: String,
    pub project: String,
    pub title: String,
    pub icon: String,
    pub version: String,
}

pub(crate) type Ledger = HashMap<String, Entry>;

pub(crate) fn ledger_path(id: &str) -> Result<PathBuf, String> {
    Ok(instance_dir(id)?.join("content.json"))
}

pub(crate) fn load_ledger(id: &str) -> Ledger {
    ledger_path(id).ok().and_then(|p| read_json(&p)).unwrap_or_default()
}

struct Pick {
    url: String,
    file: String,
    sha1: Option<String>,
    size: Option<u64>,
    version: String,
    deps: Vec<String>,
}

async fn mr_pick(project: &str, kind: &str, mc: &str, loader: &str) -> Result<Pick, String> {
    let mut params = Vec::new();
    if !mc.is_empty() {
        params.push(("game_versions", json!([mc]).to_string()));
    }
    let loaders = mr_loaders(loader, mc);
    if kind == "mod" && !loaders.is_empty() {
        params.push(("loaders", json!(loaders).to_string()));
    }
    let list = mr_get(&format!("{MODRINTH}/project/{project}/version"), &params).await?;
    let list = list.as_array().cloned().unwrap_or_default();
    let v = list.iter().find(|v| text(v, "version_type") == "release").or(list.first()).ok_or_else(|| match (mc, kind) {
        ("", _) => "That project has no files yet.".to_string(),
        (_, "mod") => format!("Nothing for Minecraft {mc} with {loader} yet."),
        _ => format!("Nothing for Minecraft {mc} yet."),
    })?;
    let files = v.get("files").and_then(Value::as_array).cloned().unwrap_or_default();
    let f = files.iter().find(|f| f.get("primary").and_then(Value::as_bool) == Some(true)).or(files.first()).ok_or("That version has no file.")?;
    let required: Vec<Value> = v
        .get("dependencies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|d| text(d, "dependency_type") == "required")
        .cloned()
        .collect();
    let mut deps: Vec<String> = Vec::new();
    for d in &required {
        match d.get("project_id").and_then(Value::as_str) {
            Some(p) => deps.push(p.to_string()),
            // Some mods name what they need only by one exact version: look up which project that is.
            None => {
                if let Some(ver) = d.get("version_id").and_then(Value::as_str) {
                    if let Ok(dv) = mr_get(&format!("{MODRINTH}/version/{ver}"), &[]).await {
                        if let Some(p) = dv.get("project_id").and_then(Value::as_str) {
                            deps.push(p.to_string());
                        }
                    }
                }
            }
        }
    }
    Ok(Pick {
        url: text(f, "url"),
        file: text(f, "filename"),
        sha1: f.pointer("/hashes/sha1").and_then(Value::as_str).map(String::from),
        size: f.get("size").and_then(Value::as_u64),
        version: text(v, "version_number"),
        deps,
    })
}

async fn cf_pick(key: &str, project: &str, kind: &str, mc: &str, loader: &str) -> Result<Pick, String> {
    let mut params: Vec<(&str, String)> = vec![("pageSize", "50".into())];
    if !mc.is_empty() {
        params.push(("gameVersion", mc.into()));
    }
    if kind == "mod" {
        if let Some(t) = cf_loader(loader) {
            params.push(("modLoaderType", t.to_string()));
        }
    }
    let v = cf_get(&format!("{CURSEFORGE}/mods/{project}/files"), key, &params).await?;
    let mut files: Vec<&Value> = v.get("data").and_then(Value::as_array).into_iter().flatten().collect();
    files.sort_by(|a, b| text(b, "fileDate").cmp(&text(a, "fileDate")));
    let f = files
        .iter()
        .find(|f| f.get("releaseType").and_then(Value::as_u64) == Some(1))
        .or(files.first())
        .copied()
        .ok_or_else(|| format!("Nothing for Minecraft {mc} yet."))?;
    let url = text(f, "downloadUrl");
    if url.is_empty() {
        // The author turned off downloads outside CurseForge's own app and site; respect that.
        return Err(format!("{}'s author only allows downloads from the CurseForge website.", text(f, "displayName")));
    }
    let sha1 = f
        .get("hashes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|h| h.get("algo").and_then(Value::as_u64) == Some(1))
        .and_then(|h| h.get("value").and_then(Value::as_str))
        .map(String::from);
    let deps = f
        .get("dependencies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|d| d.get("relationType").and_then(Value::as_u64) == Some(3))
        .filter_map(|d| d.get("modId").and_then(Value::as_u64))
        .map(|x| x.to_string())
        .collect();
    Ok(Pick { url, file: text(f, "fileName"), sha1, size: f.get("fileLength").and_then(Value::as_u64), version: text(f, "displayName"), deps })
}

/// A dependency's name and icon, for the Installed list.
async fn project_meta(key: Option<&str>, project: &str) -> Option<(String, String)> {
    match key {
        Some(k) => {
            let v = cf_get(&format!("{CURSEFORGE}/mods/{project}"), k, &[]).await.ok()?;
            Some((v.pointer("/data/name")?.as_str()?.to_string(), v.pointer("/data/logo/thumbnailUrl").and_then(Value::as_str).unwrap_or_default().to_string()))
        }
        None => {
            let v = mr_get(&format!("{MODRINTH}/project/{project}"), &[]).await.ok()?;
            Some((text(&v, "title"), text(&v, "icon_url")))
        }
    }
}

#[tauri::command]
pub async fn install_mod(instance: String, source: String, project: String, kind: String, title: String, icon: String) -> Result<Vec<String>, String> {
    let inst = load_instance(&instance)?;
    let dir = game_dir(&instance)?.join(folder(&kind)?);
    let key = if source == "curseforge" { Some(cf_key()?) } else { None };
    let mut ledger = load_ledger(&instance);
    let mut queue = vec![(valid_project(&project)?.to_string(), title, icon)];
    let mut seen = HashSet::new();
    let mut installed = Vec::new();
    let mut first = true;

    while let Some((proj, title, icon)) = queue.pop() {
        if !seen.insert(proj.clone()) || seen.len() > 40 {
            continue;
        }
        let is_root = std::mem::replace(&mut first, false);
        // The dev client already has Fabric API built in; a second copy would stop it loading.
        if !inst.dev.is_empty() && FABRIC_API.contains(&proj.as_str()) {
            if is_root {
                return Err("Fabric API is already part of the dev client.".into());
            }
            continue;
        }
        // A required mod that is already here stays as it is.
        if !is_root && ledger.values().any(|x| x.source == source && x.project == proj) {
            continue;
        }
        let pick = match &key {
            Some(k) => cf_pick(k, &proj, &kind, &inst.version, &inst.loader).await,
            None => mr_pick(&proj, &kind, &inst.version, &inst.loader).await,
        };
        let pick = match pick {
            Ok(p) => p,
            Err(err) if is_root => return Err(err),
            Err(_) => continue,
        };
        let file = safe_file(&pick.file)?;
        let job = Job { url: pick.url.clone(), path: dir.join(&file), sha1: pick.sha1.clone(), size: pick.size };
        if !net::present(&job) {
            net::fetch(&job).await?;
        }
        // An update replaces the older file of the same project.
        let old: Vec<String> = ledger.iter().filter(|(f, x)| x.source == source && x.project == proj && **f != file).map(|(f, _)| f.clone()).collect();
        for f in old {
            let _ = std::fs::remove_file(dir.join(&f));
            let _ = std::fs::remove_file(dir.join(format!("{f}.disabled")));
            ledger.remove(&f);
        }
        let (title, icon) = if title.is_empty() { project_meta(key.as_deref(), &proj).await.unwrap_or((file.clone(), String::new())) } else { (title, icon) };
        ledger.insert(file.clone(), Entry { source: source.clone(), project: proj.clone(), title, icon, version: pick.version.clone() });
        installed.push(file);
        if kind == "mod" {
            for dep in pick.deps {
                if valid_project(&dep).is_ok() {
                    queue.push((dep, String::new(), String::new()));
                }
            }
        }
    }
    write_json(&ledger_path(&instance)?, &ledger)?;
    Ok(installed)
}

#[derive(Serialize)]
pub struct Installed {
    file: String,
    kind: String,
    enabled: bool,
    title: String,
    icon: String,
    version: String,
    source: String,
    project: String,
    description: String,
    authors: String,
}

#[tauri::command(async)]
pub fn list_mods(instance: String) -> Result<Vec<Installed>, String> {
    let ledger = load_ledger(&instance);
    let base = game_dir(&instance)?;
    let mut out = Vec::new();
    for kind in ["mod", "resourcepack", "shader"] {
        for entry in std::fs::read_dir(base.join(folder(kind)?)).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let key = name.trim_end_matches(".disabled").to_string();
            if kind == "mod" && !key.to_ascii_lowercase().ends_with(".jar") {
                continue;
            }
            let info = ledger.get(&key).cloned().unwrap_or_default();
            // What the file says about itself fills in whatever the launcher has no record of.
            let own = crate::installed::meta(&entry.path(), kind);
            let pick = |a: String, b: String| if a.is_empty() { b } else { a };
            let fallback = key.trim_end_matches(".jar").trim_end_matches(".zip").to_string();
            out.push(Installed {
                title: pick(info.title, pick(own.name, fallback)),
                enabled: !name.ends_with(".disabled"),
                file: name,
                kind: kind.to_string(),
                icon: pick(info.icon, own.icon),
                version: pick(info.version, own.version),
                source: info.source,
                project: info.project,
                description: own.description,
                authors: own.authors,
            });
        }
    }
    out.sort_by_key(|x| x.title.to_lowercase());
    Ok(out)
}

fn content_path(instance: &str, kind: &str, file: &str) -> Result<PathBuf, String> {
    Ok(game_dir(instance)?.join(folder(kind)?).join(safe_file(file)?))
}

#[tauri::command]
pub fn toggle_mod(instance: String, kind: String, file: String) -> Result<(), String> {
    let from = content_path(&instance, &kind, &file)?;
    let to = match file.strip_suffix(".disabled") {
        Some(f) => from.with_file_name(f),
        None => from.with_file_name(format!("{file}.disabled")),
    };
    std::fs::rename(&from, &to).map_err(|_| "Couldn't switch it. Close Minecraft first if it's running.".to_string())
}

#[tauri::command(async)]
pub fn remove_mod(instance: String, kind: String, file: String) -> Result<(), String> {
    let path = content_path(&instance, &kind, &file)?;
    (if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) })
        .map_err(|_| "Couldn't remove it. Close Minecraft first if it's running.".to_string())?;
    let mut ledger = load_ledger(&instance);
    if ledger.remove(file.trim_end_matches(".disabled")).is_some() {
        write_json(&ledger_path(&instance)?, &ledger)?;
    }
    Ok(())
}

// ---------------------------------------------------------------- modpacks

/// Sites a Modrinth pack may download from (the .mrpack format's own allow-list).
fn pack_url_ok(url: &str) -> bool {
    url.starts_with("https://") && ["cdn.modrinth.com", "github.com", "raw.githubusercontent.com", "gitlab.com"].contains(&net::host(url))
}

/// Forge's installer wants "1.20.1-47.2.0" where a pack often says only "47.2.0"; NeoForge for 1.20.1 is the same.
fn loader_build(loader: &str, mc: &str, v: &str) -> String {
    match loader {
        "forge" if !v.starts_with(&format!("{mc}-")) => format!("{mc}-{v}"),
        "neoforge" if mc == "1.20.1" && !v.starts_with("1.20.1-") => format!("1.20.1-{v}"),
        _ => v.to_string(),
    }
}

struct PackFile {
    path: String,
    url: String,
    sha1: Option<String>,
    size: Option<u64>,
}

/// What a pack asks for: the game and loader, its files, its override folders, and files only its own site may serve.
struct Plan {
    name: String,
    mc: String,
    loader: String,
    loader_version: String,
    files: Vec<PackFile>,
    overrides: Vec<String>,
    skipped: Vec<String>,
}

fn mr_plan(index: &Value) -> Result<Plan, String> {
    let deps = index.get("dependencies").cloned().unwrap_or(Value::Null);
    let mc = text(&deps, "minecraft");
    let (loader, version) = [("fabric-loader", "fabric"), ("quilt-loader", "quilt"), ("neoforge", "neoforge"), ("forge", "forge")]
        .iter()
        .find_map(|(key, loader)| deps.get(*key).and_then(Value::as_str).map(|v| (loader.to_string(), v.to_string())))
        .unwrap_or_else(|| ("vanilla".to_string(), String::new()));
    let mut files = Vec::new();
    for f in index.get("files").and_then(Value::as_array).into_iter().flatten() {
        if f.pointer("/env/client").and_then(Value::as_str) == Some("unsupported") {
            continue;
        }
        let url = f.pointer("/downloads/0").and_then(Value::as_str).unwrap_or_default().to_string();
        if !pack_url_ok(&url) {
            return Err(format!("This pack wants a file from {}, which packs aren't allowed to use.", net::host(&url)));
        }
        files.push(PackFile {
            path: text(f, "path"),
            url,
            sha1: f.pointer("/hashes/sha1").and_then(Value::as_str).map(String::from),
            size: f.get("fileSize").and_then(Value::as_u64),
        });
    }
    Ok(Plan {
        name: text(index, "name"),
        loader_version: loader_build(&loader, &mc, &version),
        mc,
        loader,
        files,
        overrides: vec!["overrides".into(), "client-overrides".into()],
        skipped: Vec::new(),
    })
}

async fn cf_post(key: &str, path: &str, body: Value) -> Result<Value, String> {
    let resp = client()
        .post(format!("{CURSEFORGE}/{path}"))
        .header("x-api-key", key)
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("Couldn't reach CurseForge ({err})"))?;
    match resp.status().as_u16() {
        200 => resp.json().await.map_err(e),
        401 | 403 => Err("CurseForge turned the launcher's key away. Try again later.".into()),
        code => Err(format!("CurseForge answered {code}.")),
    }
}

async fn cf_plan(key: &str, manifest: &Value) -> Result<Plan, String> {
    let mc = manifest.pointer("/minecraft/version").and_then(Value::as_str).unwrap_or_default().to_string();
    let loaders = manifest.pointer("/minecraft/modLoaders").and_then(Value::as_array).cloned().unwrap_or_default();
    let primary = loaders.iter().find(|l| l.get("primary").and_then(Value::as_bool) == Some(true)).or(loaders.first()).map(|l| text(l, "id")).unwrap_or_default();
    let (loader, version) = match primary.split_once('-') {
        Some((l @ ("forge" | "neoforge" | "fabric" | "quilt"), v)) => (l.to_string(), v.to_string()),
        _ => ("vanilla".to_string(), String::new()),
    };
    let wanted: Vec<&Value> = manifest
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|f| f.get("required").and_then(Value::as_bool) != Some(false))
        .collect();
    let file_ids: Vec<u64> = wanted.iter().filter_map(|f| f.get("fileID").and_then(Value::as_u64)).collect();
    let mod_ids: Vec<u64> = wanted.iter().filter_map(|f| f.get("projectID").and_then(Value::as_u64)).collect();
    // Two requests for the whole pack: every file's details, and what kind each project is (mod, resource pack, shader).
    let files = cf_post(key, "mods/files", json!({ "fileIds": file_ids })).await?;
    let mods = cf_post(key, "mods", json!({ "modIds": mod_ids })).await?;
    let class: HashMap<u64, u64> = mods
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| Some((m.get("id")?.as_u64()?, m.get("classId")?.as_u64()?)))
        .collect();
    let mut out = Vec::new();
    let mut skipped = Vec::new();
    for f in files.get("data").and_then(Value::as_array).into_iter().flatten() {
        let name = text(f, "fileName");
        let url = text(f, "downloadUrl");
        // The author turned off downloads outside CurseForge's own app and site; respect that and list the file.
        if !url.starts_with("https://") {
            skipped.push(name);
            continue;
        }
        let dir = match f.get("modId").and_then(Value::as_u64).and_then(|id| class.get(&id).copied()) {
            Some(12) => "resourcepacks",
            Some(6552) => "shaderpacks",
            _ => "mods",
        };
        let sha1 = f
            .get("hashes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|h| h.get("algo").and_then(Value::as_u64) == Some(1))
            .and_then(|h| h.get("value").and_then(Value::as_str))
            .map(String::from);
        out.push(PackFile { path: format!("{dir}/{}", safe_file(&name)?), url, sha1, size: f.get("fileLength").and_then(Value::as_u64) });
    }
    let overrides = manifest.get("overrides").and_then(Value::as_str).unwrap_or("overrides").to_string();
    Ok(Plan {
        name: text(manifest, "name"),
        loader_version: loader_build(&loader, &mc, &version),
        mc,
        loader,
        files: out,
        overrides: vec![overrides],
        skipped,
    })
}

/// Copies a pack's override folders (configs, resource packs, worlds) into the instance; later folders win.
fn unpack_overrides(pack: &Path, game: &Path, folders: &[String]) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(pack).map_err(e)?).map_err(e)?;
    for folder in folders {
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(e)?;
            // enclosed_name refuses entries like ../../x that would land outside the instance.
            let Some(path) = entry.enclosed_name().map(|p| p.to_path_buf()) else { continue };
            let Ok(rel) = path.strip_prefix(folder) else { continue };
            if rel.as_os_str().is_empty() {
                continue;
            }
            let out = game.join(rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&out).map_err(e)?;
                continue;
            }
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(e)?;
            }
            let mut file = std::fs::File::create(&out).map_err(e)?;
            std::io::copy(&mut entry, &mut file).map_err(e)?;
        }
    }
    Ok(())
}

async fn fill_pack(app: &AppHandle, inst: &Instance, pack: &Path, plan: &Plan) -> Result<(), String> {
    let game = game_dir(&inst.id)?;
    let mut jobs = Vec::new();
    for f in &plan.files {
        jobs.push(Job { url: f.url.clone(), path: game.join(safe_rel(&f.path)?), sha1: f.sha1.clone(), size: f.size });
    }
    let (app2, id2) = (app.clone(), inst.id.clone());
    net::download_all(jobs, move |done, total| progress(&app2, &id2, "Downloading the modpack", done, total)).await?;
    unpack_overrides(pack, &game, &plan.overrides)
}

#[derive(Serialize)]
pub struct PackResult {
    instance: Instance,
    skipped: Vec<String>,
}

/// A .mrpack already on this PC (one a friend shared from their instance screen) becomes its own instance.
pub(crate) async fn install_local_pack(app: &AppHandle, pack: &Path, title: String) -> Result<PackResult, String> {
    let index = zip_entry(pack, "modrinth.index.json").map_err(|_| "That isn't a modpack file (.mrpack).".to_string())?;
    let plan = mr_plan(&serde_json::from_slice::<Value>(&index).map_err(e)?)?;
    if plan.mc.is_empty() {
        return Err("This modpack doesn't say which Minecraft version it's for.".into());
    }
    let name = if title.trim().is_empty() { plan.name.clone() } else { title };
    let inst = create_instance(name, plan.mc.clone(), plan.loader.clone(), plan.loader_version.clone())?;
    if let Err(err) = fill_pack(app, &inst, pack, &plan).await {
        let _ = std::fs::remove_dir_all(instance_dir(&inst.id)?);
        return Err(err);
    }
    progress(app, &inst.id, "Ready", 0, 0);
    Ok(PackResult { instance: inst, skipped: plan.skipped })
}

/// A modpack becomes its own instance: the pack's game and loader, its files, then its override folders.
/// Minecraft itself still downloads on the first Play, like any instance.
#[tauri::command]
pub async fn install_modpack(app: AppHandle, source: String, project: String, title: String) -> Result<PackResult, String> {
    let project = valid_project(&project)?.to_string();
    let key = if source == "curseforge" { Some(cf_key()?) } else { None };
    let pick = match &key {
        Some(k) => cf_pick(k, &project, "modpack", "", "").await?,
        None => mr_pick(&project, "modpack", "", "").await?,
    };
    let pack = root().join("cache").join("modpacks").join(safe_file(&pick.file)?);
    let job = Job { url: pick.url.clone(), path: pack.clone(), sha1: pick.sha1.clone(), size: pick.size };
    if !net::present(&job) {
        net::fetch(&job).await?;
    }
    let plan = match &key {
        Some(k) => cf_plan(k, &serde_json::from_slice::<Value>(&zip_entry(&pack, "manifest.json")?).map_err(e)?).await?,
        None => mr_plan(&serde_json::from_slice::<Value>(&zip_entry(&pack, "modrinth.index.json")?).map_err(e)?)?,
    };
    if plan.mc.is_empty() {
        return Err("This modpack doesn't say which Minecraft version it's for.".into());
    }
    let name = if title.trim().is_empty() { plan.name.clone() } else { title };
    let inst = create_instance(name, plan.mc.clone(), plan.loader.clone(), plan.loader_version.clone())?;
    if let Err(err) = fill_pack(&app, &inst, &pack, &plan).await {
        // A half-made instance is no use, and it holds nothing of the player's yet.
        let _ = std::fs::remove_dir_all(instance_dir(&inst.id)?);
        return Err(err);
    }
    progress(&app, &inst.id, "Ready", 0, 0);
    Ok(PackResult { instance: inst, skipped: plan.skipped })
}
