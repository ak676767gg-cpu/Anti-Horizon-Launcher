// Minecraft: Java Edition itself: Mojang's version list, instances, installing (the game, its libraries and assets,
// the right Java) and launching. Game files live in one shared store, so ten instances of a version download it once.
use crate::console;
use crate::net::{self, Job};
use crate::store::{self, e, read_json, root, text, write_json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const JAVA_RUNTIMES: &str =
    "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
const RESOURCES: &str = "https://resources.download.minecraft.net/";
const LIBRARIES: &str = "https://libraries.minecraft.net/";
const LOADERS: [&str; 5] = ["vanilla", "fabric", "quilt", "forge", "neoforge"];

/// Aikar's widely used G1 settings: fewer, shorter garbage-collector pauses, so frame times stay even.
const TUNED_GC: [&str; 16] = [
    "-XX:+UnlockExperimentalVMOptions",
    "-XX:+UseG1GC",
    "-XX:+ParallelRefProcEnabled",
    "-XX:MaxGCPauseMillis=200",
    "-XX:+DisableExplicitGC",
    "-XX:G1NewSizePercent=30",
    "-XX:G1MaxNewSizePercent=40",
    "-XX:G1HeapRegionSize=8M",
    "-XX:G1ReservePercent=20",
    "-XX:G1HeapWastePercent=5",
    "-XX:G1MixedGCCountTarget=4",
    "-XX:InitiatingHeapOccupancyPercent=15",
    "-XX:G1MixedGCLiveThresholdPercent=90",
    "-XX:SurvivorRatio=32",
    "-XX:MaxTenuringThreshold=1",
    "-XX:+PerfDisableSharedMem",
];

static BUSY: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Running games: the instance id and its Java process id.
static RUNNING: Mutex<Vec<(String, u32)>> = Mutex::new(Vec::new());
/// Games the player force-closed, so their exit isn't reported as a crash.
static STOPPED: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// One install at a time, so two instances of the same version never write the same file together.
static INSTALL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A name used as a folder (an instance or version id). Refuses anything that could point elsewhere on disk.
pub fn safe_name(s: &str) -> Result<&str, String> {
    let bad = s.is_empty()
        || s.len() > 96
        || s.starts_with('.')
        || s.chars().any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'));
    if bad {
        Err(format!("\"{s}\" can't be used as a name."))
    } else {
        Ok(s)
    }
}

/// A relative path from downloaded metadata, refused if it could climb out of its folder.
pub(crate) fn safe_rel(rel: &str) -> Result<&str, String> {
    let bad = rel.is_empty() || rel.starts_with(['/', '\\']) || rel.contains(':') || rel.split(['/', '\\']).any(|p| p == "..");
    if bad {
        Err(format!("Refused an unsafe path in the game files: {rel}"))
    } else {
        Ok(rel)
    }
}

fn opt(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from)
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect()
}

fn p2s(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn version_json(id: &str) -> PathBuf {
    root().join("versions").join(id).join(format!("{id}.json"))
}

fn version_jar(id: &str) -> PathBuf {
    root().join("versions").join(id).join(format!("{id}.jar"))
}

pub fn instance_dir(id: &str) -> Result<PathBuf, String> {
    Ok(root().join("instances").join(safe_name(id)?))
}

pub fn game_dir(id: &str) -> Result<PathBuf, String> {
    let dir = instance_dir(id)?;
    // The dev client plays in its project's own run folder, so mods installed here are the ones it loads.
    if let Some(inst) = read_json::<Instance>(&dir.join("instance.json")) {
        if !inst.dev.is_empty() {
            return Ok(PathBuf::from(inst.dev).join("run"));
        }
    }
    Ok(dir.join("minecraft"))
}

pub(crate) fn progress(app: &AppHandle, id: &str, stage: &str, done: usize, total: usize) {
    let _ = app.emit("progress", json!({ "id": id, "stage": stage, "done": done, "total": total }));
}

fn is_running(id: &str) -> bool {
    RUNNING.lock().unwrap().iter().any(|(x, _)| x == id)
}

/// Marks an instance as being set up until dropped, so a double click never starts two installs.
struct Busy(String);

impl Busy {
    fn take(id: &str) -> Result<Busy, String> {
        let mut busy = BUSY.lock().unwrap();
        if busy.iter().any(|x| x == id) {
            return Err("This instance is already getting ready.".into());
        }
        busy.push(id.to_string());
        Ok(Busy(id.to_string()))
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        BUSY.lock().unwrap().retain(|x| x != &self.0);
    }
}

pub fn no_window(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

// ---------------------------------------------------------------- versions

#[derive(Serialize, Deserialize, Clone)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    #[serde(default)]
    pub sha1: String,
    #[serde(rename = "releaseTime", default)]
    pub release_time: String,
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    latest: HashMap<String, String>,
    versions: Vec<VersionEntry>,
}

/// Fresh when online, the last good copy when not.
async fn cached_json(url: &str, cache: &Path) -> Result<Value, String> {
    match net::get_json::<Value>(url).await {
        Ok(v) => {
            let _ = write_json(cache, &v);
            Ok(v)
        }
        Err(err) => read_json(cache).ok_or(err),
    }
}

async fn manifest() -> Result<Manifest, String> {
    let v = cached_json(MANIFEST, &root().join("cache").join("version_manifest_v2.json")).await?;
    serde_json::from_value(v).map_err(e)
}

#[derive(Serialize)]
pub struct VersionList {
    latest_release: String,
    latest_snapshot: String,
    versions: Vec<VersionEntry>,
}

#[tauri::command]
pub async fn get_versions() -> Result<VersionList, String> {
    let m = manifest().await?;
    Ok(VersionList {
        latest_release: m.latest.get("release").cloned().unwrap_or_default(),
        latest_snapshot: m.latest.get("snapshot").cloned().unwrap_or_default(),
        versions: m.versions,
    })
}

/// Numbers inside a loader build ("1.20.1-47.2.0" -> 1,20,1,47,2,0), for newest-first sorting.
fn version_key(s: &str) -> Vec<u64> {
    s.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).map(|p| p.parse().unwrap_or(0)).collect()
}

/// NeoForge numbers its builds after the Minecraft version: 1.21.1 -> 21.1.x, 1.21 -> 21.0.x.
fn neoforge_prefix(mc: &str) -> String {
    match mc.strip_prefix("1.") {
        Some(rest) => {
            let mut p: Vec<&str> = rest.split('.').collect();
            if p.len() == 1 {
                p.push("0");
            }
            format!("{}.{}.", p[0], p[1])
        }
        None => format!("{mc}."),
    }
}

#[tauri::command]
pub async fn get_loader_versions(loader: String, mc_version: String) -> Result<Vec<String>, String> {
    let mc = safe_name(&mc_version)?.to_string();
    let mut list: Vec<String> = match loader.as_str() {
        "fabric" | "quilt" => {
            let url = if loader == "fabric" {
                format!("https://meta.fabricmc.net/v2/versions/loader/{mc}")
            } else {
                format!("https://meta.quiltmc.org/v3/versions/loader/{mc}")
            };
            let v: Vec<Value> = net::get_json(&url).await?;
            // Already newest first, with stable builds marked by the loader itself.
            return Ok(v.iter().filter_map(|x| x.pointer("/loader/version").and_then(Value::as_str).map(String::from)).collect());
        }
        "forge" => {
            let v: Value = net::get_json("https://files.minecraftforge.net/net/minecraftforge/forge/maven-metadata.json").await?;
            strings(v.get(&mc))
        }
        "neoforge" if mc == "1.20.1" => {
            let v: Value = net::get_json("https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/forge").await?;
            strings(v.get("versions")).into_iter().filter(|s| s.starts_with("1.20.1-")).collect()
        }
        "neoforge" => {
            let prefix = neoforge_prefix(&mc);
            let v: Value = net::get_json("https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge").await?;
            strings(v.get("versions")).into_iter().filter(|s| s.starts_with(&prefix)).collect()
        }
        _ => Vec::new(),
    };
    list.sort_by(|a, b| version_key(b).cmp(&version_key(a)));
    Ok(list)
}

// ---------------------------------------------------------------- instances

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub version: String,
    pub loader: String,
    pub loader_version: String,
    /// The version id that actually launches (a Fabric or Forge profile), filled in by the first install.
    pub profile: String,
    pub created: u64,
    pub last_played: u64,
    pub play_seconds: u64,
    /// A Fabric development project this instance runs instead of a downloaded game (the Anti-Horizon client).
    pub dev: String,
    /// The preset it was made from (Presets page), if any.
    pub preset: String,
}

#[derive(Serialize)]
pub struct InstanceView {
    #[serde(flatten)]
    inst: Instance,
    running: bool,
}

pub fn load_instance(id: &str) -> Result<Instance, String> {
    let mut inst: Instance =
        read_json(&instance_dir(id)?.join("instance.json")).ok_or_else(|| "That instance no longer exists.".to_string())?;
    inst.id = id.to_string();
    Ok(inst)
}

pub(crate) fn save_instance(inst: &Instance) -> Result<(), String> {
    write_json(&instance_dir(&inst.id)?.join("instance.json"), inst)
}

#[tauri::command]
pub fn list_instances() -> Vec<InstanceView> {
    #[cfg(debug_assertions)]
    offer_dev_instance();
    let running: Vec<String> = RUNNING.lock().unwrap().iter().map(|(x, _)| x.clone()).collect();
    let mut list: Vec<Instance> = std::fs::read_dir(root().join("instances"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|d| {
            let mut inst: Instance = read_json(&d.path().join("instance.json"))?;
            inst.id = d.file_name().to_string_lossy().into_owned();
            Some(inst)
        })
        .collect();
    list.sort_by_key(|i| std::cmp::Reverse(i.last_played.max(i.created)));
    list.into_iter().map(|inst| InstanceView { running: running.contains(&inst.id), inst }).collect()
}

#[tauri::command]
pub fn create_instance(name: String, version: String, loader: String, loader_version: String) -> Result<Instance, String> {
    safe_name(&version)?;
    if !LOADERS.contains(&loader.as_str()) {
        return Err("Pick a mod loader.".into());
    }
    let loader_version = if loader == "vanilla" { String::new() } else { safe_name(&loader_version)?.to_string() };
    let label = match loader.as_str() {
        "fabric" => "Fabric",
        "quilt" => "Quilt",
        "forge" => "Forge",
        "neoforge" => "NeoForge",
        _ => "Minecraft",
    };
    let name: String = match name.trim() {
        "" => format!("{label} {version}"),
        n => n.chars().take(48).collect(),
    };
    let slug: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let slug: String = slug.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-").chars().take(40).collect();
    let slug = if slug.is_empty() { "instance".to_string() } else { slug };
    let mut id = slug.clone();
    let mut n = 2;
    while root().join("instances").join(&id).exists() {
        id = format!("{slug}-{n}");
        n += 1;
    }
    let inst = Instance { id, name, version, loader, loader_version, created: now(), ..Default::default() };
    std::fs::create_dir_all(game_dir(&inst.id)?).map_err(e)?;
    save_instance(&inst)?;
    crate::presets::preinstall(&inst);
    Ok(inst)
}

/// Into the Recycle Bin rather than gone for good: an instance holds the player's worlds.
#[cfg(windows)]
pub(crate) fn to_recycle_bin(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW,
    };
    let mut from: Vec<u16> = path.as_os_str().encode_wide().collect();
    from.extend([0, 0]);
    // SAFETY: an all-zero SHFILEOPSTRUCTW is valid; `from` is double-null terminated and outlives the call.
    let code = unsafe {
        let mut op: SHFILEOPSTRUCTW = std::mem::zeroed();
        op.wFunc = FO_DELETE as _;
        op.pFrom = from.as_ptr();
        op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT) as _;
        let code = SHFileOperationW(&mut op);
        if op.fAnyOperationsAborted != 0 { -1 } else { code }
    };
    if code == 0 {
        Ok(())
    } else {
        Err(format!("Windows couldn't move the instance to the Recycle Bin (code {code})."))
    }
}

#[cfg(not(windows))]
pub(crate) fn to_recycle_bin(path: &Path) -> Result<(), String> {
    std::fs::remove_dir_all(path).map_err(e)
}

#[tauri::command(async)]
pub fn delete_instance(id: String) -> Result<(), String> {
    if is_running(&id) || BUSY.lock().unwrap().iter().any(|x| x == &id) {
        return Err("Close Minecraft before deleting this instance.".into());
    }
    let dir = instance_dir(&id)?;
    if !dir.join("instance.json").exists() {
        return Err("That instance no longer exists.".into());
    }
    to_recycle_bin(&dir)
}

#[tauri::command]
pub fn open_instance_folder(id: String, sub: String) -> Result<(), String> {
    let base = game_dir(&id)?;
    let dir = match sub.as_str() {
        "" => base,
        "mods" | "resourcepacks" | "shaderpacks" | "saves" | "logs" | "screenshots" => base.join(&sub),
        _ => return Err("Unknown folder.".into()),
    };
    std::fs::create_dir_all(&dir).map_err(e)?;
    store::open_path(&dir);
    Ok(())
}

// ---------------------------------------------------------------- version files

async fn vanilla_json(id: &str) -> Result<Value, String> {
    let m = manifest().await?;
    let entry = m.versions.iter().find(|v| v.id == id).ok_or_else(|| format!("Minecraft {id} isn't in Mojang's version list."))?;
    let job = Job { url: entry.url.clone(), path: version_json(id), sha1: Some(entry.sha1.clone()).filter(|s| !s.is_empty()), size: None };
    net::fetch(&job).await?;
    read_json(&job.path).ok_or_else(|| format!("The file for Minecraft {id} couldn't be read."))
}

/// A profile and the versions it builds on (a Fabric profile on its Minecraft version, and so on), top first.
async fn version_chain(top: &str) -> Result<Vec<Value>, String> {
    let mut chain = Vec::new();
    let mut next = Some(top.to_string());
    while let Some(id) = next {
        if chain.len() > 5 {
            return Err("These version files refer to each other in a loop.".into());
        }
        safe_name(&id)?;
        let v = match read_json::<Value>(&version_json(&id)) {
            Some(v) => v,
            None => vanilla_json(&id).await?,
        };
        next = v.get("inheritsFrom").and_then(Value::as_str).map(String::from);
        chain.push(v);
    }
    Ok(chain)
}

/// Folds a profile onto the versions it builds on: its libraries come first (they win), its arguments add on.
fn merge(mut chain: Vec<Value>) -> Value {
    let mut merged = chain.pop().unwrap_or(Value::Null);
    while let Some(child) = chain.pop() {
        let mut out = merged.as_object().cloned().unwrap_or_default();
        for (k, v) in child.as_object().cloned().unwrap_or_default() {
            if k == "inheritsFrom" {
                continue;
            } else if k == "libraries" {
                let mut libs = v.as_array().cloned().unwrap_or_default();
                libs.extend(out.get("libraries").and_then(Value::as_array).cloned().unwrap_or_default());
                out.insert(k, Value::Array(libs));
            } else if k == "arguments" {
                let mut args = out.get("arguments").and_then(Value::as_object).cloned().unwrap_or_default();
                for side in ["game", "jvm"] {
                    let mut list = args.get(side).and_then(Value::as_array).cloned().unwrap_or_default();
                    list.extend(v.get(side).and_then(Value::as_array).cloned().unwrap_or_default());
                    args.insert(side.to_string(), Value::Array(list));
                }
                out.insert(k, Value::Object(args));
            } else {
                out.insert(k, v);
            }
        }
        merged = Value::Object(out);
    }
    merged
}

fn os_matches(os: &Value) -> bool {
    let name = match os.get("name").and_then(Value::as_str) {
        None | Some("windows") => true,
        Some("windows-arm64") => cfg!(target_arch = "aarch64"),
        Some(_) => false,
    };
    let arch = match os.get("arch").and_then(Value::as_str) {
        None => true,
        Some("x86") => cfg!(target_arch = "x86"),
        Some("arm64") | Some("aarch64") => cfg!(target_arch = "aarch64"),
        Some("x86_64") | Some("x64") => cfg!(target_arch = "x86_64"),
        Some(_) => false,
    };
    name && arch
}

/// Mojang's allow/disallow rules: the last rule that matches this PC decides; no rules means allowed.
fn allowed(rules: Option<&Value>) -> bool {
    let Some(rules) = rules.and_then(Value::as_array) else { return true };
    let mut ok = false;
    for rule in rules {
        // Optional features (demo mode, a custom window size, quick play) are all off.
        let matches = rule.get("os").map_or(true, os_matches)
            && rule.get("features").and_then(Value::as_object).map_or(true, |f| f.values().all(|v| v.as_bool() == Some(false)));
        if matches {
            ok = rule.get("action").and_then(Value::as_str) == Some("allow");
        }
    }
    ok
}

fn arg_list(v: &Value, side: &str) -> Vec<String> {
    let mut out = Vec::new();
    for a in v.pointer(&format!("/arguments/{side}")).and_then(Value::as_array).into_iter().flatten() {
        match a {
            Value::String(s) => out.push(s.clone()),
            Value::Object(o) if allowed(o.get("rules")) => match o.get("value") {
                Some(Value::String(s)) => out.push(s.clone()),
                Some(Value::Array(list)) => out.extend(list.iter().filter_map(|x| x.as_str().map(String::from))),
                _ => {}
            },
            _ => {}
        }
    }
    out
}

/// Fills ${name} placeholders in one pass, so a value can never bring in another placeholder.
fn fill(s: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find('}') {
            Some(len) => {
                let key = &rest[start + 2..start + 2 + len];
                match vars.get(key) {
                    Some(v) => out.push_str(v),
                    None => out.push_str(&rest[start..start + 3 + len]),
                }
                rest = &rest[start + 3 + len..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------- libraries, assets, Java

struct Lib {
    key: String,
    path: PathBuf,
    /// None for files a loader's installer makes on the spot (Forge's patched game).
    job: Option<Job>,
    /// Some(excluded prefixes) for the old natives jars that get unpacked rather than put on the classpath.
    extract: Option<Vec<String>>,
}

/// group:artifact[:classifier], the identity of a library whatever its version.
fn lib_key(name: &str) -> String {
    let p: Vec<&str> = name.split('@').next().unwrap_or(name).split(':').collect();
    match p.len() {
        0..=2 => name.to_string(),
        3 => format!("{}:{}", p[0], p[1]),
        _ => format!("{}:{}:{}", p[0], p[1], p[3]),
    }
}

/// Where a Maven coordinate lives: group/artifact/version/artifact-version[-classifier].ext
fn maven_path(name: &str) -> Option<String> {
    let (coords, ext) = name.split_once('@').unwrap_or((name, "jar"));
    let p: Vec<&str> = coords.split(':').collect();
    if p.len() < 3 {
        return None;
    }
    let file = match p.get(3) {
        Some(c) => format!("{}-{}-{}.{ext}", p[1], p[2], c),
        None => format!("{}-{}.{ext}", p[1], p[2]),
    };
    Some(format!("{}/{}/{}/{file}", p[0].replace('.', "/"), p[1], p[2]))
}

fn lib_entry(dir: &Path, key: String, rel: Option<String>, url: Option<String>, sha1: Option<String>, size: Option<u64>, extract: Option<Vec<String>>) -> Result<Option<Lib>, String> {
    let Some(rel) = rel else { return Ok(None) };
    let path = dir.join(safe_rel(&rel)?);
    let job = url.filter(|u| !u.is_empty()).map(|url| Job { url, path: path.clone(), sha1, size });
    Ok(Some(Lib { key, path, job, extract }))
}

fn libraries(v: &Value) -> Result<Vec<Lib>, String> {
    let dir = root().join("libraries");
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for lib in v.get("libraries").and_then(Value::as_array).into_iter().flatten() {
        if !allowed(lib.get("rules")) {
            continue;
        }
        let Some(name) = lib.get("name").and_then(Value::as_str) else { continue };
        let downloads = lib.get("downloads");
        let natives = lib.get("natives");
        let maven_base = lib.get("url").and_then(Value::as_str).unwrap_or(LIBRARIES).trim_end_matches('/').to_string();
        let artifact = downloads.and_then(|d| d.get("artifact"));

        // The library itself, for the classpath. The first copy wins, so a loader's newer build beats the game's.
        if (artifact.is_some() || (downloads.is_none() && natives.is_none())) && seen.insert(lib_key(name)) {
            let entry = match artifact {
                Some(a) => lib_entry(&dir, lib_key(name), opt(a, "path").or_else(|| maven_path(name)), opt(a, "url"), opt(a, "sha1"), a.get("size").and_then(Value::as_u64), None)?,
                None => {
                    let rel = maven_path(name);
                    let url = rel.as_ref().map(|r| format!("{maven_base}/{r}"));
                    lib_entry(&dir, lib_key(name), rel, url, opt(lib, "sha1"), lib.get("size").and_then(Value::as_u64), None)?
                }
            };
            out.extend(entry);
        }

        // Natives the old way (before 1.19): a separate jar per system, unpacked next to the game.
        if let Some(classifier) = natives.and_then(|n| n.get("windows")).and_then(Value::as_str) {
            let classifier = classifier.replace("${arch}", if cfg!(target_pointer_width = "32") { "32" } else { "64" });
            let exclude = strings(lib.pointer("/extract/exclude"));
            let key = format!("{}:{classifier}", lib_key(name));
            let entry = match downloads.and_then(|d| d.get("classifiers")).and_then(|c| c.get(&classifier)) {
                Some(a) => lib_entry(&dir, key, opt(a, "path"), opt(a, "url"), opt(a, "sha1"), a.get("size").and_then(Value::as_u64), Some(exclude))?,
                None => {
                    let rel = maven_path(&format!("{name}:{classifier}"));
                    let url = rel.as_ref().map(|r| format!("{maven_base}/{r}"));
                    lib_entry(&dir, key, rel, url, None, None, Some(exclude))?
                }
            };
            out.extend(entry);
        }
    }
    Ok(out)
}

async fn asset_jobs(v: &Value, jobs: &mut Vec<Job>) -> Result<(String, Value), String> {
    let ai = v.get("assetIndex").ok_or("This version has no asset list.")?;
    let id = opt(ai, "id").unwrap_or_else(|| "legacy".into());
    let path = root().join("assets").join("indexes").join(format!("{}.json", safe_name(&id)?));
    let job = Job { url: text(ai, "url"), path: path.clone(), sha1: opt(ai, "sha1"), size: ai.get("size").and_then(Value::as_u64) };
    if !net::present(&job) {
        net::fetch(&job).await?;
    }
    let index: Value = read_json(&path).ok_or("The asset list couldn't be read.")?;
    let objects = root().join("assets").join("objects");
    for obj in index.get("objects").and_then(Value::as_object).into_iter().flat_map(|m| m.values()) {
        let hash = text(obj, "hash");
        if hash.len() < 3 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let pre = &hash[..2];
        jobs.push(Job {
            url: format!("{RESOURCES}{pre}/{hash}"),
            path: objects.join(pre).join(&hash),
            sha1: Some(hash.clone()),
            size: obj.get("size").and_then(Value::as_u64),
        });
    }
    Ok((id, index))
}

fn platform() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "windows-arm64"
    } else if cfg!(target_arch = "x86") {
        "windows-x86"
    } else {
        "windows-x64"
    }
}

/// Mojang's own Java build for a version (the same one the official launcher uses), queued for download.
async fn java_jobs(component: &str, jobs: &mut Vec<Job>) -> Result<PathBuf, String> {
    safe_name(component)?;
    let all = cached_json(JAVA_RUNTIMES, &root().join("cache").join("java-runtimes.json")).await?;
    let entry = [platform(), "windows-x64"]
        .iter()
        .find_map(|p| all.get(*p).and_then(|x| x.get(component)).and_then(|x| x.get(0)))
        .ok_or_else(|| format!("Mojang has no Java ({component}) for this PC."))?;
    let m = entry.get("manifest").ok_or("The Java listing has no file list.")?;
    let list = root().join("runtime").join("manifests").join(format!("{component}.json"));
    let want = opt(m, "sha1");
    let fresh = std::fs::read(&list).map(|b| Some(net::sha1_hex(&b)) == want).unwrap_or(false);
    if !fresh {
        net::fetch(&Job { url: text(m, "url"), path: list.clone(), sha1: want, size: None }).await?;
    }
    let files: Value = read_json(&list).ok_or("The Java file list couldn't be read.")?;
    let dir = root().join("runtime").join(component);
    for (rel, f) in files.get("files").and_then(Value::as_object).into_iter().flatten() {
        let rel = safe_rel(rel)?;
        match f.get("type").and_then(Value::as_str) {
            Some("file") => {
                if let Some(raw) = f.pointer("/downloads/raw") {
                    jobs.push(Job { url: text(raw, "url"), path: dir.join(rel), sha1: opt(raw, "sha1"), size: raw.get("size").and_then(Value::as_u64) });
                }
            }
            Some("directory") => {
                let _ = std::fs::create_dir_all(dir.join(rel));
            }
            _ => {}
        }
    }
    Ok(dir.join("bin").join("javaw.exe"))
}

fn unpack_natives(jar: &Path, dir: &Path, exclude: &[String]) -> Result<(), String> {
    let file = std::fs::File::open(jar).map_err(|err| format!("{}: {err}", jar.display()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(e)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(e)?;
        if entry.is_dir() || exclude.iter().any(|x| entry.name().starts_with(x.as_str())) {
            continue;
        }
        // enclosed_name refuses entries like ../../x that would land outside the folder.
        let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else { continue };
        let out = dir.join(rel);
        if std::fs::metadata(&out).map(|m| m.len() == entry.size()).unwrap_or(false) {
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(e)?;
        }
        let mut f = std::fs::File::create(&out).map_err(e)?;
        std::io::copy(&mut entry, &mut f).map_err(e)?;
    }
    Ok(())
}

/// Very old versions read their sounds and textures from a plain folder tree instead of the hashed store.
fn copy_assets(index: &Value, to: &Path) -> Result<(), String> {
    let objects = root().join("assets").join("objects");
    for (rel, obj) in index.get("objects").and_then(Value::as_object).into_iter().flatten() {
        let hash = text(obj, "hash");
        if hash.len() < 3 {
            continue;
        }
        let out = to.join(safe_rel(rel)?);
        if out.exists() {
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(e)?;
        }
        std::fs::copy(objects.join(&hash[..2]).join(&hash), &out).map_err(e)?;
    }
    Ok(())
}

// ---------------------------------------------------------------- installing

/// Everything one profile needs, downloaded and ready to launch.
struct Prepared {
    top: String,
    merged: Value,
    java: PathBuf,
    java_major: u64,
    classpath: Vec<PathBuf>,
    natives: PathBuf,
    asset_index: String,
    game_assets: PathBuf,
    log_arg: Option<String>,
}

async fn prepare(app: &AppHandle, id: &str, top: &str) -> Result<Prepared, String> {
    progress(app, id, "Checking game files", 0, 0);
    let chain = version_chain(top).await?;
    let base = chain.last().map(|v| text(v, "id")).unwrap_or_default();
    let merged = merge(chain);
    let mut jobs = Vec::new();

    // Java: Mojang's own build for this version, unless Settings names one.
    let java_path = store::load_settings().java_path.trim().to_string();
    let java_major = merged.pointer("/javaVersion/majorVersion").and_then(Value::as_u64).unwrap_or(8);
    let java = if java_path.is_empty() {
        let component = merged.pointer("/javaVersion/component").and_then(Value::as_str).unwrap_or("jre-legacy").to_string();
        java_jobs(&component, &mut jobs).await?
    } else {
        PathBuf::from(java_path)
    };

    // The game itself.
    let jar_id = merged.get("jar").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or(base.as_str()).to_string();
    safe_name(&jar_id)?;
    if let Some(c) = merged.pointer("/downloads/client") {
        jobs.push(Job { url: text(c, "url"), path: version_jar(&jar_id), sha1: opt(c, "sha1"), size: c.get("size").and_then(Value::as_u64) });
    }

    let libs = libraries(&merged)?;
    jobs.extend(libs.iter().filter_map(|l| l.job.clone()));
    let (asset_index, index) = asset_jobs(&merged, &mut jobs).await?;

    let mut log_arg = None;
    if let (Some(arg), Some(file)) = (merged.pointer("/logging/client/argument").and_then(Value::as_str), merged.pointer("/logging/client/file")) {
        let path = root().join("assets").join("log_configs").join(safe_name(&text(file, "id"))?);
        jobs.push(Job { url: text(file, "url"), path: path.clone(), sha1: opt(file, "sha1"), size: file.get("size").and_then(Value::as_u64) });
        log_arg = Some(arg.replace("${path}", &p2s(&path)));
    }

    let (app2, id2) = (app.clone(), id.to_string());
    net::download_all(jobs, move |done, total| progress(&app2, &id2, "Downloading game files", done, total)).await?;

    let natives = instance_dir(id)?.join("natives");
    std::fs::create_dir_all(&natives).map_err(e)?;
    for lib in &libs {
        if let Some(exclude) = &lib.extract {
            unpack_natives(&lib.path, &natives, exclude)?;
        }
    }

    let mut game_assets = root().join("assets");
    if index.get("map_to_resources").and_then(Value::as_bool) == Some(true) {
        game_assets = game_dir(id)?.join("resources");
        copy_assets(&index, &game_assets)?;
    } else if index.get("virtual").and_then(Value::as_bool) == Some(true) {
        game_assets = root().join("assets").join("virtual").join(&asset_index);
        copy_assets(&index, &game_assets)?;
    }

    // The game jar under the profile's own name, as Mojang's launcher keeps it (Forge relies on that name).
    let main_jar = version_jar(top);
    if top != jar_id && !main_jar.exists() {
        if let Some(dir) = main_jar.parent() {
            std::fs::create_dir_all(dir).map_err(e)?;
        }
        if std::fs::hard_link(version_jar(&jar_id), &main_jar).is_err() {
            std::fs::copy(version_jar(&jar_id), &main_jar).map_err(e)?;
        }
    }
    if let Some(missing) = libs.iter().find(|l| !l.path.exists()) {
        return Err(format!("A game file is missing ({}). Press Play again to repair it.", missing.key));
    }
    let mut classpath: Vec<PathBuf> = libs.iter().filter(|l| l.extract.is_none()).map(|l| l.path.clone()).collect();
    classpath.push(main_jar);
    Ok(Prepared { top: top.to_string(), merged, java, java_major, classpath, natives, asset_index, game_assets, log_arg })
}

/// Fabric and Quilt publish a ready-made profile per game and loader version.
async fn loader_profile(inst: &Instance) -> Result<String, String> {
    let (mc, lv) = (safe_name(&inst.version)?, safe_name(&inst.loader_version)?);
    let (id, url) = if inst.loader == "fabric" {
        (format!("fabric-loader-{lv}-{mc}"), format!("https://meta.fabricmc.net/v2/versions/loader/{mc}/{lv}/profile/json"))
    } else {
        (format!("quilt-loader-{lv}-{mc}"), format!("https://meta.quiltmc.org/v3/versions/loader/{mc}/{lv}/profile/json"))
    };
    if version_json(&id).exists() {
        return Ok(id);
    }
    let v: Value = net::get_json(&url).await?;
    let id = opt(&v, "id").unwrap_or(id);
    write_json(&version_json(safe_name(&id)?), &v)?;
    Ok(id)
}

pub(crate) fn zip_entry(jar: &Path, name: &str) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(jar).map_err(e)?;
    let mut zip = zip::ZipArchive::new(file).map_err(e)?;
    let mut entry = zip.by_name(name).map_err(|_| format!("{name} is missing from the installer."))?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).map_err(e)?;
    Ok(buf)
}

/// Files Forge's installer builds itself (the patched game); if any is gone the installer runs again.
async fn missing_generated(id: &str) -> Result<bool, String> {
    let merged = merge(version_chain(id).await?);
    Ok(libraries(&merged)?.iter().any(|l| l.job.is_none() && !l.path.exists()))
}

/// Forge and NeoForge patch the game with their own installer, run headless into the launcher's folder.
async fn forge_install(app: &AppHandle, inst: &Instance, java: &Path) -> Result<String, String> {
    let v = safe_name(&inst.loader_version)?.to_string();
    let url = match inst.loader.as_str() {
        "forge" => format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{v}/forge-{v}-installer.jar"),
        _ if v.starts_with("1.20.1-") => format!("https://maven.neoforged.net/releases/net/neoforged/forge/{v}/forge-{v}-installer.jar"),
        _ => format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{v}/neoforge-{v}-installer.jar"),
    };
    let jar = root().join("cache").join("installers").join(format!("{}-{v}-installer.jar", inst.loader));
    let job = Job { url, path: jar.clone(), sha1: None, size: None };
    if !net::present(&job) {
        progress(app, &inst.id, "Downloading the installer", 0, 0);
        net::fetch(&job).await?;
    }
    let profile: Value = serde_json::from_slice(&zip_entry(&jar, "version.json")?).map_err(e)?;
    let id = opt(&profile, "id").ok_or("The installer has no version id.")?;
    safe_name(&id)?;
    if version_json(&id).exists() && !missing_generated(&id).await? {
        return Ok(id);
    }

    // The installer looks for a launcher profile file; ours stands in for the official launcher's.
    let profiles = root().join("launcher_profiles.json");
    if !profiles.exists() {
        std::fs::write(&profiles, b"{\"profiles\":{}}").map_err(e)?;
    }
    let name = if inst.loader == "forge" { "Forge" } else { "NeoForge" };
    progress(app, &inst.id, &format!("Running the {name} installer"), 0, 0);
    let (java, target) = (java.to_path_buf(), root());
    let output = tauri::async_runtime::spawn_blocking(move || {
        let mut cmd = std::process::Command::new(&java);
        cmd.arg("-Djava.awt.headless=true").arg("-jar").arg(&jar).arg("--installClient").arg(&target).current_dir(&target);
        no_window(&mut cmd);
        cmd.output()
    })
    .await
    .map_err(e)?
    .map_err(|err| format!("Java couldn't start the installer: {err}"))?;
    if !output.status.success() || !version_json(&id).exists() {
        let log = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        let tail: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).collect();
        let tail = tail[tail.len().saturating_sub(4)..].join(" / ");
        return Err(format!("The {name} installer stopped: {tail}"));
    }
    Ok(id)
}

async fn install(app: &AppHandle, inst: &mut Instance) -> Result<Prepared, String> {
    let _one_at_a_time = INSTALL.lock().await;
    let profile = match inst.loader.as_str() {
        "fabric" | "quilt" => loader_profile(inst).await?,
        "forge" | "neoforge" => {
            let installed = safe_name(&inst.profile).is_ok() && version_json(&inst.profile).exists() && !missing_generated(&inst.profile).await?;
            if installed {
                inst.profile.clone()
            } else {
                // The installer needs the plain game and its Java in place first.
                let base = prepare(app, &inst.id, &inst.version).await?;
                forge_install(app, inst, &base.java).await?
            }
        }
        _ => inst.version.clone(),
    };
    let ready = prepare(app, &inst.id, &profile).await?;
    if inst.profile != profile {
        inst.profile = profile;
        save_instance(inst)?;
    }
    Ok(ready)
}

#[tauri::command]
pub async fn install_instance(app: AppHandle, id: String) -> Result<(), String> {
    let _busy = Busy::take(&id)?;
    let mut inst = load_instance(&id)?;
    if !inst.dev.is_empty() {
        return Err("The dev client builds from its own project when you press Play.".into());
    }
    let result = install(&app, &mut inst).await;
    progress(&app, &id, if result.is_ok() { "Ready" } else { "Stopped" }, 0, 0);
    result.map(|_| ())
}

// ---------------------------------------------------------------- launching

/// Ends a game that won't close itself. Anything since its last autosave is lost, so the window asks first.
#[tauri::command]
pub fn stop_instance(id: String) -> Result<(), String> {
    let pid = RUNNING.lock().unwrap().iter().find(|(x, _)| x == &id).map(|(_, pid)| *pid).ok_or("That instance isn't running.")?;
    let dev = load_instance(&id).map(|i| i.dev).unwrap_or_default();
    STOPPED.lock().unwrap().push(id);
    // The development copy can run the client project, whose game runs under Gradle, outside this process tree.
    #[cfg(debug_assertions)]
    let game = !dev.is_empty() && kill_dev_game(&dev);
    #[cfg(not(debug_assertions))]
    let game = {
        let _ = dev; // only the development copy runs a client project
        false
    };
    let mut cmd = std::process::Command::new("taskkill");
    cmd.args(["/PID", &pid.to_string(), "/T", "/F"]);
    no_window(&mut cmd);
    match cmd.status() {
        Ok(status) if status.success() => Ok(()),
        _ if game => Ok(()),
        _ => Err("Windows couldn't close the game.".into()),
    }
}

#[tauri::command]
pub async fn launch_instance(app: AppHandle, id: String) -> Result<(), String> {
    if is_running(&id) {
        return Err("This instance is already running.".into());
    }
    let _busy = Busy::take(&id)?;
    let result = launch(&app, &id).await;
    if result.is_err() {
        progress(&app, &id, "Stopped", 0, 0);
    }
    result
}

/// The Java command for a prepared profile: JVM options, the main class and the game's own arguments.
fn command(p: &Prepared, account: &crate::auth::Account, settings: &store::Settings, game: &Path) -> Result<(Vec<String>, String, Vec<String>), String> {
    let classpath = p.classpath.iter().map(|x| p2s(x)).collect::<Vec<_>>().join(";");
    let vars: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", account.name.clone()),
        ("auth_uuid", account.uuid.clone()),
        ("auth_access_token", account.mc_token.clone()),
        ("auth_session", format!("token:{}:{}", account.mc_token, account.uuid)),
        ("auth_xuid", account.xuid.clone()),
        ("clientid", crate::auth::CLIENT_ID.to_string()),
        ("user_type", "msa".to_string()),
        ("user_properties", "{}".to_string()),
        ("version_name", p.top.clone()),
        ("version_type", p.merged.get("type").and_then(Value::as_str).unwrap_or("release").to_string()),
        ("game_directory", p2s(game)),
        ("assets_root", p2s(&root().join("assets"))),
        ("game_assets", p2s(&p.game_assets)),
        ("assets_index_name", p.asset_index.clone()),
        ("natives_directory", p2s(&p.natives)),
        ("library_directory", p2s(&root().join("libraries"))),
        ("classpath", classpath),
        ("classpath_separator", ";".to_string()),
        ("launcher_name", "anti-horizon-launcher".to_string()),
        ("launcher_version", env!("CARGO_PKG_VERSION").to_string()),
    ]);

    let mem = settings.memory_mb.clamp(1024, 65536);
    let mut jvm = vec![format!("-Xmx{mem}M"), format!("-Xms{}M", (mem / 2).max(512))];
    if settings.tuned_gc {
        jvm.extend(TUNED_GC.iter().map(|s| s.to_string()));
    }
    let template = if p.merged.pointer("/arguments/jvm").is_some() {
        arg_list(&p.merged, "jvm")
    } else {
        vec!["-Djava.library.path=${natives_directory}".into(), "-cp".into(), "${classpath}".into()]
    };
    jvm.extend(template.iter().map(|a| fill(a, &vars)));
    jvm.extend(settings.jvm_args.split_whitespace().map(String::from));
    if let Some(arg) = &p.log_arg {
        jvm.push(arg.clone());
    }

    let game_args: Vec<String> = if p.merged.pointer("/arguments/game").is_some() {
        arg_list(&p.merged, "game").iter().map(|a| fill(a, &vars)).collect()
    } else {
        p.merged.get("minecraftArguments").and_then(Value::as_str).unwrap_or_default().split_whitespace().map(|a| fill(a, &vars)).collect()
    };
    let main_class = p.merged.get("mainClass").and_then(Value::as_str).ok_or("This version has no main class.")?.to_string();
    Ok((jvm, main_class, game_args))
}

async fn launch(app: &AppHandle, id: &str) -> Result<(), String> {
    let mut inst = load_instance(id)?;
    if !inst.dev.is_empty() {
        // Account-free launching of the client project exists only in the development copy, never in a release.
        #[cfg(debug_assertions)]
        return launch_dev(app, id, inst);
        #[cfg(not(debug_assertions))]
        return Err("This is a development instance. It only runs in the launcher's development copy.".into());
    }
    let account = crate::auth::account_for_launch().await?;
    let settings = store::load_settings();
    console::reset(app, id);
    console::note(app, id, &format!("Getting {} ready", inst.name));
    let p = install(app, &mut inst).await?;
    progress(app, id, "Starting Minecraft", 0, 0);
    console::note(app, id, &format!("Starting Minecraft {} with Java {}", p.top, p.java_major));

    let game = game_dir(id)?;
    std::fs::create_dir_all(game.join("logs")).map_err(e)?;
    crate::presets::ensure_client(app, &inst, &game, settings.client_everywhere).await;
    // Anything the mods need that isn't installed, so the game doesn't stop at "Incompatible mods found".
    if inst.loader == "fabric" || inst.loader == "quilt" {
        progress(app, id, "Checking what the mods need", 0, 0);
        if let Ok(f) = crate::deps::fix(id).await {
            if !f.added.is_empty() {
                console::note(app, id, &format!("Added what the mods need: {}", f.added.join(", ")));
            }
            if !f.unknown.is_empty() {
                console::note(app, id, &format!("Not found on Modrinth, so not added: {} (Minecraft may say these are missing)", f.unknown.join(", ")));
            }
        }
    }
    if settings.quiet_jei {
        crate::tweaks::quiet_jei(&game);
    }
    let (mut jvm, main_class, game_args) = command(&p, &account, &settings, &game)?;
    // Newer versions point Java, JNA, LWJGL and Netty at sub-folders of the natives folder; make sure they exist.
    let natives = p2s(&p.natives);
    for arg in &jvm {
        if let Some((_, value)) = arg.split_once('=') {
            if value.starts_with(&natives) {
                let _ = std::fs::create_dir_all(value);
            }
        }
    }

    // A modded classpath can pass Windows' 32,767-character command limit; Java 9+ reads it from a file instead.
    if p.java_major >= 9 {
        if let Some(i) = jvm.iter().position(|a| a == "-cp" || a == "-classpath") {
            if let Some(cp) = jvm.get(i + 1).cloned() {
                let file = instance_dir(id)?.join("classpath.txt");
                std::fs::write(&file, format!("-cp \"{}\"\n", cp.replace('\\', "/"))).map_err(e)?;
                jvm.splice(i..=i + 1, [format!("@{}", file.display())]);
            }
        }
    }


    let log = std::fs::File::create(game.join("logs").join("launcher-output.log")).map_err(e)?;
    let mut cmd = std::process::Command::new(&p.java);
    cmd.args(&jvm)
        .arg(&main_class)
        .args(&game_args)
        .current_dir(&game)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    no_window(&mut cmd);
    let mut child = cmd.spawn().map_err(|err| format!("Java didn't start ({}): {err}", p.java.display()))?;
    console::pipe(app, id, &mut child, log);
    watch(app, id, inst, child, settings.minimize_on_launch)
}

/// Marks a started game as running, then waits for it on a thread and reports when it closes.
fn watch(app: &AppHandle, id: &str, mut inst: Instance, mut child: std::process::Child, minimize: bool) -> Result<(), String> {
    inst.last_played = now();
    save_instance(&inst)?;
    RUNNING.lock().unwrap().push((id.to_string(), child.id()));
    let _ = app.emit("game", json!({ "id": id, "running": true }));
    progress(app, id, "Playing", 0, 0);
    if minimize {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.minimize();
        }
    }

    let (app2, id2) = (app.clone(), id.to_string());
    let what = if inst.dev.is_empty() { "Minecraft" } else { "The dev client" };
    std::thread::spawn(move || {
        let started = Instant::now();
        let code = child.wait().ok().and_then(|s| s.code());
        RUNNING.lock().unwrap().retain(|(x, _)| x != &id2);
        let forced = {
            let mut stopped = STOPPED.lock().unwrap();
            let was = stopped.contains(&id2);
            stopped.retain(|x| x != &id2);
            was
        };
        if let Ok(mut inst) = load_instance(&id2) {
            inst.play_seconds += started.elapsed().as_secs();
            let _ = save_instance(&inst);
        }
        let note = match (code, forced) {
            (_, true) => "Force closed".to_string(),
            (Some(c), false) if c != 0 => format!("{what} stopped with code {c}: the lines above say why"),
            _ => format!("{what} closed"),
        };
        console::note(&app2, &id2, &note);
        let _ = app2.emit("game", json!({ "id": id2, "running": false, "code": code, "forced": forced }));
        if let Some(w) = app2.get_webview_window("main") {
            let _ = w.unminimize();
        }
    });
    Ok(())
}

// ---------------------------------------------------------------- the dev client
// Only in the development copy (debug builds): runs the Anti-Horizon client's own Fabric project for testing it.
// Release builds, the ones people install, leave all of this out.

#[cfg(debug_assertions)]
const DEV_ID: &str = "anti-horizon-client-dev";

#[cfg(debug_assertions)]
/// The Anti-Horizon client's Fabric project on this PC, if there is one.
fn dev_project() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("USERPROFILE")?).join("anti-horizon-client");
    let ok = dir.join("gradlew.bat").is_file() && dir.join("src").join("main").join("resources").join("fabric.mod.json").is_file();
    ok.then_some(dir)
}

#[cfg(debug_assertions)]
/// Adds an instance for the client project the first time the launcher finds it. Deleting it keeps it gone.
fn offer_dev_instance() {
    let marker = root().join("dev-offered");
    if marker.exists() {
        return;
    }
    let Some(project) = dev_project() else { return };
    let props = std::fs::read_to_string(project.join("gradle.properties")).unwrap_or_default();
    let prop = |key: &str| {
        props
            .lines()
            .find_map(|l| l.trim().strip_prefix(key)?.trim_start().strip_prefix('=').map(|v| v.trim().to_string()))
            .unwrap_or_default()
    };
    let inst = Instance {
        id: DEV_ID.into(),
        name: "Anti-Horizon Client (dev)".into(),
        version: prop("minecraft_version"),
        loader: "fabric".into(),
        loader_version: prop("loader_version"),
        dev: p2s(&project),
        created: now(),
        ..Default::default()
    };
    if save_instance(&inst).is_ok() {
        let _ = std::fs::write(marker, b"");
    }
}

#[cfg(debug_assertions)]
/// The JDK the project builds with: the one its run-client.bat sets, or JAVA_HOME.
fn dev_java_home(project: &Path) -> Result<String, String> {
    let bat = std::fs::read_to_string(project.join("run-client.bat")).unwrap_or_default();
    let from_bat = bat.lines().find_map(|l| {
        let l = l.trim();
        let rest = l.strip_prefix("set \"JAVA_HOME=").or_else(|| l.strip_prefix("set JAVA_HOME="))?;
        Some(rest.trim_end_matches('"').to_string())
    });
    from_bat
        .into_iter()
        .chain(std::env::var("JAVA_HOME").ok())
        .find(|home| Path::new(home).join("bin").join("java.exe").is_file())
        .ok_or_else(|| "The dev client needs a JDK: set JAVA_HOME in its run-client.bat.".into())
}

#[cfg(debug_assertions)]
/// Runs the project's own `gradlew runClient`, which compiles the latest code first. No account is used: this is
/// Fabric's development setup, for testing the client on this PC.
fn launch_dev(app: &AppHandle, id: &str, inst: Instance) -> Result<(), String> {
    let project = PathBuf::from(&inst.dev);
    if !project.join("gradlew.bat").is_file() {
        return Err(format!("The client project isn't at {} any more.", project.display()));
    }
    let java_home = dev_java_home(&project)?;
    let logs = project.join("run").join("logs");
    std::fs::create_dir_all(&logs).map_err(e)?;
    console::reset(app, id);
    console::note(app, id, &format!("Building the client from {}, then starting it", project.display()));
    progress(app, id, "Building the client", 0, 0);
    let log = std::fs::File::create(logs.join("launcher-output.log")).map_err(e)?;
    // By its full path: Windows can be set to never run programs from the current folder by name alone.
    let mut cmd = std::process::Command::new(project.join("gradlew.bat"));
    cmd.args(["runClient", "--console=plain"])
        .current_dir(&project)
        .env("JAVA_HOME", &java_home)
        .env_remove("NoDefaultCurrentDirectoryInExePath")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    no_window(&mut cmd);
    let mut child = cmd.spawn().map_err(|err| format!("Gradle didn't start: {err}"))?;
    console::pipe(app, id, &mut child, log);
    // Stays up while testing, so the console can be watched next to the game.
    watch(app, id, inst, child, false)
}

#[cfg(debug_assertions)]
/// The dev game runs under Gradle's background daemon, outside the launcher's process tree, so it is found by the
/// project it was started from.
fn kill_dev_game(project: &str) -> bool {
    let pattern = format!("*fabric.dli.config={}*", project.replace('\'', "''"));
    let script = format!(
        "$n = 0; Get-CimInstance Win32_Process -Filter \"Name='java.exe' or Name='javaw.exe'\" | Where-Object {{ $_.CommandLine -like '{pattern}' }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force; $n++ }}; if ($n -eq 0) {{ exit 1 }}"
    );
    let mut cmd = std::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    no_window(&mut cmd);
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Builds the whole Java command for real version files (Mojang's and Fabric's) without downloading the game.
    // Needs the internet: cargo test -- --ignored --nocapture
    #[test]
    #[ignore]
    fn commands_for_real_versions() {
        let dir = std::env::temp_dir().join("ah-launcher-test");
        std::env::set_var("APPDATA", &dir);
        tauri::async_runtime::block_on(async {
            let fabric = Instance { loader: "fabric".into(), version: "26.3".into(), loader_version: "0.20.1".into(), ..Default::default() };
            let fabric = loader_profile(&fabric).await.unwrap();
            for top in ["26.3", fabric.as_str(), "1.12.2", "1.8.9", "b1.7.3"] {
                let merged = merge(version_chain(top).await.unwrap());
                let libs = libraries(&merged).unwrap();
                let mut keys = HashSet::new();
                for lib in libs.iter().filter(|l| l.extract.is_none()) {
                    assert!(keys.insert(lib.key.clone()), "{top}: {} is on the classpath twice", lib.key);
                }
                let mut classpath: Vec<PathBuf> = libs.iter().filter(|l| l.extract.is_none()).map(|l| l.path.clone()).collect();
                classpath.push(version_jar(top));
                let p = Prepared {
                    top: top.to_string(),
                    java: PathBuf::from("javaw.exe"),
                    java_major: merged.pointer("/javaVersion/majorVersion").and_then(Value::as_u64).unwrap_or(8),
                    classpath,
                    natives: dir.join("natives"),
                    asset_index: merged.pointer("/assetIndex/id").and_then(Value::as_str).unwrap_or_default().to_string(),
                    game_assets: dir.join("assets"),
                    log_arg: None,
                    merged,
                };
                let account = crate::auth::Account { uuid: "0123456789abcdef0123456789abcdef".into(), name: "Tester".into(), xuid: "42".into(), mc_token: "token".into(), ..Default::default() };
                let (jvm, main, game) = command(&p, &account, &store::Settings::default(), &dir.join("game")).unwrap();
                let shown: Vec<&String> = jvm.iter().filter(|a| !a.starts_with("-XX") && !a.contains(';')).collect();
                let natives = libs.iter().filter(|l| l.extract.is_some()).count();
                println!("\n{top}: {} libraries ({natives} native jars to unpack), Java {}\n  main: {main}\n  jvm: {shown:?}\n  game: {game:?}", libs.len(), p.java_major);
                let unfilled: Vec<&String> = jvm.iter().chain(&game).filter(|a| a.contains("${")).collect();
                assert!(unfilled.is_empty(), "{top}: placeholders left {unfilled:?}");
                assert!(jvm.iter().any(|a| a == "-cp"), "{top}: no classpath");
                if top == "1.8.9" {
                    assert!(natives > 0, "1.8.9 should have natives to unpack");
                }
            }
        });
    }
}
