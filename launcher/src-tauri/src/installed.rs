// What is installed in an instance. Every file is described by what it says about itself (a mod's
// fabric.mod.json, quilt.mod.json or mods.toml; a pack's pack.mcmeta), so even mods added by hand show their real
// name, version and icon. Files are also recognised on Modrinth by their SHA-1, which gives them a project page and
// lets them update, whoever put them there.
use crate::mc::{game_dir, load_instance};
use crate::mods::{folder, ledger_path, load_ledger, mr_get, mr_loaders, safe_file, Entry, Ledger, FABRIC_API, MODRINTH};
use crate::net::{self, client, Job};
use crate::store::{e, text, write_json};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;
use zip::ZipArchive;

/// What a file says about itself.
#[derive(Clone, Default)]
pub struct Meta {
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: String,
    /// A data: URL of the file's own icon, when it has a small enough PNG.
    pub icon: String,
}

type MetaCache = HashMap<PathBuf, (u64, Option<SystemTime>, Meta)>;
static META: LazyLock<Mutex<MetaCache>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Read once per file version: the cache is keyed by size and modified time, so a replaced file is read again.
pub fn meta(path: &Path, kind: &str) -> Meta {
    let Ok(md) = std::fs::metadata(path) else { return Meta::default() };
    if !md.is_file() {
        return Meta::default();
    }
    let stamp = (md.len(), md.modified().ok());
    if let Some((len, time, m)) = META.lock().unwrap().get(path) {
        if (*len, *time) == stamp {
            return m.clone();
        }
    }
    let m = read_meta(path, kind).unwrap_or_default();
    META.lock().unwrap().insert(path.to_path_buf(), (stamp.0, stamp.1, m.clone()));
    m
}

fn read_meta(path: &Path, kind: &str) -> Option<Meta> {
    let mut zip = ZipArchive::new(File::open(path).ok()?).ok()?;
    match kind {
        "mod" => fabric(&mut zip).or_else(|| quilt(&mut zip)).or_else(|| forge(&mut zip)),
        _ => pack(&mut zip),
    }
}

fn entry(zip: &mut ZipArchive<File>, name: &str, limit: u64) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name.trim_start_matches('/')).ok()?;
    if f.size() > limit {
        return None;
    }
    let mut buf = Vec::with_capacity(f.size() as usize);
    f.read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn icon(zip: &mut ZipArchive<File>, path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    match entry(zip, path, 64 * 1024) {
        Some(b) if b.starts_with(b"\x89PNG") => format!("data:image/png;base64,{}", base64(&b)),
        _ => String::new(),
    }
}

/// Mod metadata is hand-written and sometimes has raw line breaks inside strings, which strict JSON refuses.
fn lenient_json(raw: &[u8]) -> Option<Value> {
    serde_json::from_slice(raw).ok().or_else(|| {
        let text = String::from_utf8_lossy(raw).replace(['\r', '\n', '\t'], " ");
        serde_json::from_str(&text).ok()
    })
}

fn names(v: Option<&Value>) -> String {
    let list: Vec<String> = match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(String::from).or_else(|| x.get("name").and_then(Value::as_str).map(String::from)))
            .collect(),
        Some(Value::Object(m)) => m.keys().cloned().collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    };
    list.into_iter().take(3).collect::<Vec<_>>().join(", ")
}

/// An icon field is a path, or sizes mapped to paths (the largest is best).
fn icon_path(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(m)) => m
            .iter()
            .max_by_key(|(size, _)| size.parse::<u32>().unwrap_or(0))
            .and_then(|(_, p)| p.as_str())
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

fn fabric(zip: &mut ZipArchive<File>) -> Option<Meta> {
    let v = lenient_json(&entry(zip, "fabric.mod.json", 1 << 20)?)?;
    let name = Some(text(&v, "name")).filter(|s| !s.is_empty()).unwrap_or_else(|| text(&v, "id"));
    let path = icon_path(v.get("icon"));
    Some(Meta { name, version: text(&v, "version"), description: text(&v, "description"), authors: names(v.get("authors")), icon: icon(zip, &path) })
}

fn quilt(zip: &mut ZipArchive<File>) -> Option<Meta> {
    let v = lenient_json(&entry(zip, "quilt.mod.json", 1 << 20)?)?;
    let q = v.get("quilt_loader")?;
    let m = q.get("metadata").cloned().unwrap_or(Value::Null);
    let name = Some(text(&m, "name")).filter(|s| !s.is_empty()).unwrap_or_else(|| text(q, "id"));
    let path = icon_path(m.get("icon"));
    Some(Meta { name, version: text(q, "version"), description: text(&m, "description"), authors: names(m.get("contributors")), icon: icon(zip, &path) })
}

fn forge(zip: &mut ZipArchive<File>) -> Option<Meta> {
    let raw = entry(zip, "META-INF/neoforge.mods.toml", 1 << 20).or_else(|| entry(zip, "META-INF/mods.toml", 1 << 20))?;
    let toml = String::from_utf8_lossy(&raw).into_owned();
    let mods = toml.find("[[mods]]").map(|i| &toml[i..]).unwrap_or(&toml);
    let mut version = toml_value(mods, "version").unwrap_or_default();
    if version.contains("${") {
        // "${file.jarVersion}" means the jar's manifest has it.
        let manifest = entry(zip, "META-INF/MANIFEST.MF", 64 * 1024).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        version = manifest.lines().find_map(|l| l.strip_prefix("Implementation-Version:")).map(|v| v.trim().to_string()).unwrap_or_default();
    }
    let path = toml_value(mods, "logoFile").or_else(|| toml_value(&toml, "logoFile")).unwrap_or_default();
    Some(Meta {
        name: toml_value(mods, "displayName").or_else(|| toml_value(mods, "modId")).unwrap_or_default(),
        version,
        description: toml_value(mods, "description").map(|d| d.split_whitespace().collect::<Vec<_>>().join(" ")).unwrap_or_default(),
        authors: toml_value(mods, "authors").unwrap_or_default(),
        icon: icon(zip, &path),
    })
}

/// The value of `key = ...` in TOML: a "basic", 'literal' or '''multi-line''' string. Enough for mods.toml.
fn toml_value(src: &str, key: &str) -> Option<String> {
    for (i, line) in src.lines().enumerate() {
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix(key) else { continue };
        let Some(rest) = rest.trim_start().strip_prefix('=') else { continue };
        let rest = rest.trim_start();
        for q in ["'''", "\"\"\""] {
            if let Some(body) = rest.strip_prefix(q) {
                let whole = std::iter::once(body).chain(src.lines().skip(i + 1)).collect::<Vec<_>>().join("\n");
                return whole.find(q).map(|end| whole[..end].trim().to_string());
            }
        }
        for q in ['"', '\''] {
            if let Some(body) = rest.strip_prefix(q) {
                return body.find(q).map(|end| body[..end].to_string());
            }
        }
        return None;
    }
    None
}

fn pack(zip: &mut ZipArchive<File>) -> Option<Meta> {
    let v = lenient_json(&entry(zip, "pack.mcmeta", 256 * 1024)?)?;
    let description = flatten_text(v.pointer("/pack/description"));
    Some(Meta { description, icon: icon(zip, "pack.png"), ..Meta::default() })
}

/// A pack description can be plain text or a chat component; either way, the words without colour codes.
fn flatten_text(v: Option<&Value>) -> String {
    fn walk(v: &Value, out: &mut String) {
        match v {
            Value::String(s) => out.push_str(s),
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(m) => {
                if let Some(t) = m.get("text") {
                    walk(t, out);
                }
                if let Some(x) = m.get("extra") {
                    walk(x, out);
                }
            }
            _ => {}
        }
    }
    let mut raw = String::new();
    if let Some(v) = v {
        walk(v, &mut raw);
    }
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { A[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { A[n as usize & 63] as char } else { '=' });
    }
    out
}

// ---------------------------------------------------------------- Modrinth: recognising files and updates

struct Local {
    file: String,
    kind: &'static str,
    sha1: String,
}

/// Every installed file with its SHA-1, read on a worker thread (a big modpack is hundreds of megabytes).
async fn local_files(base: PathBuf) -> Result<Vec<Local>, String> {
    tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        for kind in ["mod", "resourcepack", "shader"] {
            let Ok(dir) = folder(kind) else { continue };
            for item in std::fs::read_dir(base.join(dir)).into_iter().flatten().flatten() {
                let path = item.path();
                if !path.is_file() {
                    continue;
                }
                let file = item.file_name().to_string_lossy().into_owned();
                let plain = file.trim_end_matches(".disabled").to_ascii_lowercase();
                let wanted = if kind == "mod" { plain.ends_with(".jar") } else { plain.ends_with(".zip") };
                if !wanted {
                    continue;
                }
                if let Ok(bytes) = std::fs::read(&path) {
                    out.push(Local { file, kind, sha1: net::sha1_hex(&bytes) });
                }
            }
        }
        out
    })
    .await
    .map_err(e)
}

async fn mr_post(url: &str, body: &Value) -> Result<Value, String> {
    let resp = client().post(url).json(body).send().await.map_err(|err| format!("Couldn't reach Modrinth ({err})"))?;
    match resp.status().as_u16() {
        200 => resp.json().await.map_err(e),
        429 => Err("Modrinth asked us to slow down. Try again in a minute.".into()),
        code => Err(format!("Modrinth answered {code}.")),
    }
}

/// Gives files the launcher has no record of (added by hand, or by an older launcher) their Modrinth project, title
/// and icon, when Modrinth knows them. Returns whether anything was learned.
async fn identify(files: &[Local], ledger: &mut Ledger) -> Result<bool, String> {
    let unknown: Vec<&Local> = files.iter().filter(|f| !ledger.contains_key(f.file.trim_end_matches(".disabled"))).collect();
    if unknown.is_empty() {
        return Ok(false);
    }
    let hashes: Vec<&str> = unknown.iter().map(|f| f.sha1.as_str()).collect();
    let versions = mr_post(&format!("{MODRINTH}/version_files"), &json!({ "hashes": hashes, "algorithm": "sha1" })).await?;
    let ids: HashSet<String> = unknown.iter().filter_map(|f| versions.get(&f.sha1).map(|v| text(v, "project_id"))).filter(|s| !s.is_empty()).collect();
    if ids.is_empty() {
        return Ok(false);
    }
    let ids_json = serde_json::to_string(&ids.iter().collect::<Vec<_>>()).map_err(e)?;
    let resp = client().get(format!("{MODRINTH}/projects")).query(&[("ids", ids_json)]).send().await.map_err(e)?;
    let projects: Value = resp.json().await.map_err(e)?;
    let info: HashMap<String, (String, String)> = projects
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| (text(p, "id"), (text(p, "title"), text(p, "icon_url"))))
        .collect();
    let mut learned = false;
    for f in unknown {
        let Some(v) = versions.get(&f.sha1) else { continue };
        let project = text(v, "project_id");
        let Some((title, icon)) = info.get(&project) else { continue };
        ledger.insert(
            f.file.trim_end_matches(".disabled").to_string(),
            Entry { source: "modrinth".into(), project, title: title.clone(), icon: icon.clone(), version: text(v, "version_number") },
        );
        learned = true;
    }
    Ok(learned)
}

/// A newer file for an installed one, as Modrinth lists it.
struct Newer {
    file: String,
    kind: &'static str,
    project: String,
    version: String,
    url: String,
    name: String,
    sha1: String,
    size: Option<u64>,
}

/// How settled a release channel is: a file never moves to a less settled one than it is on.
fn channel(v: &Value) -> u8 {
    match v.get("version_type").and_then(Value::as_str) {
        Some("release") => 2,
        Some("beta") => 1,
        _ => 0,
    }
}

fn primary(v: &Value) -> Option<&Value> {
    let files = v.get("files")?.as_array()?;
    files.iter().find(|x| x.get("primary").and_then(Value::as_bool) == Some(true)).or(files.first())
}

/// Files with a newer version for this instance's Minecraft version and loader. `only` limits it to some files.
///
/// Modrinth's quick answer is the very latest version, whatever its channel. A mod on a full release is only offered
/// a newer full release (a beta of Sodium is not an update for most people), and only when it really is newer.
async fn newer(instance: &str, files: &[Local], only: Option<&HashSet<String>>) -> Result<Vec<Newer>, String> {
    let inst = load_instance(instance)?;
    let mut out = Vec::new();
    for kind in ["mod", "resourcepack", "shader"] {
        let group: Vec<&Local> = files.iter().filter(|f| f.kind == kind && only.is_none_or(|o| o.contains(&f.file))).collect();
        if group.is_empty() {
            continue;
        }
        let loaders: Vec<&str> = match kind {
            "mod" => mr_loaders(&inst.loader, &inst.version),
            "resourcepack" => vec!["minecraft"],
            _ => vec!["iris", "optifine", "canvas", "vanilla"],
        };
        if loaders.is_empty() {
            continue;
        }
        let hashes: Vec<&str> = group.iter().map(|f| f.sha1.as_str()).collect();
        let current = mr_post(&format!("{MODRINTH}/version_files"), &json!({ "hashes": hashes, "algorithm": "sha1" })).await?;
        let body = json!({ "hashes": hashes, "algorithm": "sha1", "loaders": loaders, "game_versions": [inst.version] });
        let latest = mr_post(&format!("{MODRINTH}/version_files/update"), &body).await?;
        for f in group {
            let (Some(now), Some(top)) = (current.get(&f.sha1), latest.get(&f.sha1)) else { continue };
            let project = text(top, "project_id");
            // The dev client has Fabric API built in; it never gets a copy in its mods folder.
            if !inst.dev.is_empty() && FABRIC_API.contains(&project.as_str()) {
                continue;
            }
            let pick = if channel(top) >= channel(now) {
                Some(top.clone())
            } else {
                // The latest is a beta or alpha: look down the list for the newest on this file's channel or better.
                let params = [("loaders", serde_json::to_string(&loaders).map_err(e)?), ("game_versions", json!([inst.version]).to_string())];
                let list = mr_get(&format!("{MODRINTH}/project/{project}/version"), &params).await.unwrap_or(Value::Null);
                list.as_array().and_then(|a| a.iter().find(|v| channel(v) >= channel(now)).cloned())
            };
            let Some(v) = pick else { continue };
            // ISO dates compare as text; only a later version is an update.
            if text(&v, "date_published") <= text(now, "date_published") {
                continue;
            }
            let Some(file) = primary(&v) else { continue };
            let sha1 = file.pointer("/hashes/sha1").and_then(Value::as_str).unwrap_or_default().to_string();
            if sha1.is_empty() || sha1 == f.sha1 {
                continue;
            }
            out.push(Newer {
                file: f.file.clone(),
                kind,
                project,
                version: text(&v, "version_number"),
                url: text(file, "url"),
                name: text(file, "filename"),
                sha1,
                size: file.get("size").and_then(Value::as_u64),
            });
        }
    }
    Ok(out)
}

#[derive(Serialize)]
pub struct Update {
    file: String,
    kind: String,
    version: String,
}

/// Recognises unknown files on Modrinth, then lists the files that have a newer version.
#[tauri::command]
pub async fn check_updates(instance: String) -> Result<Vec<Update>, String> {
    let files = local_files(game_dir(&instance)?).await?;
    let mut ledger = load_ledger(&instance);
    if identify(&files, &mut ledger).await.unwrap_or(false) {
        write_json(&ledger_path(&instance)?, &ledger)?;
    }
    Ok(newer(&instance, &files, None)
        .await?
        .into_iter()
        .map(|n| Update { file: n.file, kind: n.kind.to_string(), version: n.version })
        .collect())
}

/// Replaces the given files with their newest versions. Each new file is downloaded and checked before the old one
/// goes, and a file that was switched off stays off.
#[tauri::command]
pub async fn update_mods(instance: String, files: Vec<String>) -> Result<usize, String> {
    let base = game_dir(&instance)?;
    let only: HashSet<String> = files.into_iter().collect();
    let local = local_files(base.clone()).await?;
    let list = newer(&instance, &local, Some(&only)).await?;
    let mut ledger = load_ledger(&instance);
    let mut done = 0;
    let mut failed: Vec<String> = Vec::new();
    for n in list {
        let dir = base.join(folder(n.kind)?);
        let name = safe_file(&n.name)?;
        let off = n.file.ends_with(".disabled");
        let old = dir.join(&n.file);
        let job = Job { url: n.url.clone(), path: dir.join(&name), sha1: Some(n.sha1.clone()), size: n.size };
        let old_key = n.file.trim_end_matches(".disabled").to_string();
        let result: Result<(), String> = async {
            net::fetch(&job).await?;
            if old != job.path {
                std::fs::remove_file(&old).map_err(|_| "the old file is in use".to_string())?;
            }
            if off {
                std::fs::rename(&job.path, dir.join(format!("{name}.disabled"))).map_err(e)?;
            }
            Ok(())
        }
        .await;
        match result {
            Ok(()) => {
                let before = ledger.remove(&old_key).unwrap_or_default();
                let fallback = meta(&dir.join(if off { format!("{name}.disabled") } else { name.clone() }), n.kind).name;
                let title = if before.title.is_empty() { fallback } else { before.title };
                ledger.insert(name.clone(), Entry { source: "modrinth".into(), project: n.project, title, icon: before.icon, version: n.version });
                done += 1;
            }
            Err(_) => failed.push(old_key),
        }
    }
    write_json(&ledger_path(&instance)?, &ledger)?;
    if !failed.is_empty() {
        let names = failed.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
        return Err(format!("Updated {done}, but {} couldn't be replaced ({names}). Close Minecraft and try again.", failed.len()));
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::{base64, flatten_text, toml_value};
    use serde_json::json;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn mods_toml_values() {
        let toml = "modLoader=\"javafml\"\nlogoFile=\"logo.png\"\n[[mods]]\nmodId=\"jei\"\nversion=\"${file.jarVersion}\"\ndisplayName = 'Just Enough Items'\ndescription='''\nShows items\nand recipes\n'''\n";
        assert_eq!(toml_value(toml, "displayName").as_deref(), Some("Just Enough Items"));
        assert_eq!(toml_value(toml, "logoFile").as_deref(), Some("logo.png"));
        assert_eq!(toml_value(toml, "description").as_deref(), Some("Shows items\nand recipes"));
        assert_eq!(toml_value(toml, "version").as_deref(), Some("${file.jarVersion}"));
        assert_eq!(toml_value(toml, "missing"), None);
    }

    #[test]
    fn pack_descriptions_lose_colour_codes() {
        assert_eq!(flatten_text(Some(&json!("§6Faithful §r32x"))), "Faithful 32x");
        assert_eq!(flatten_text(Some(&json!([{ "text": "Fresh " }, { "text": "Animations", "color": "gold" }]))), "Fresh Animations");
    }
}
