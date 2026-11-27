// The instance screen (double-click an instance): its data packs, files and worlds, and sharing it as a .mrpack file.
use crate::mc::{game_dir, load_instance, safe_rel};
use crate::mods::safe_file;
use crate::net::client;
use crate::store::{e, open_path};
use serde::Serialize;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tauri::AppHandle;

fn modified(p: &Path) -> u64 {
    std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
}

// ---------------------------------------------------------------- data packs

/// A data pack in one of the instance's worlds; shaped like the mod list's rows so the window treats them alike.
#[derive(Serialize)]
pub struct DataPack {
    file: String,
    kind: &'static str,
    enabled: bool,
    title: String,
    icon: String,
    version: String,
    source: String,
    project: String,
    description: String,
    authors: String,
}

/// Every world's data packs (saves/<world>/datapacks), named "<world>/<pack>".
#[tauri::command(async)]
pub fn list_datapacks(instance: String) -> Result<Vec<DataPack>, String> {
    let saves = game_dir(&instance)?.join("saves");
    let mut out = Vec::new();
    for world in std::fs::read_dir(&saves).into_iter().flatten().flatten() {
        let wname = world.file_name().to_string_lossy().into_owned();
        for pack in std::fs::read_dir(world.path().join("datapacks")).into_iter().flatten().flatten() {
            let name = pack.file_name().to_string_lossy().into_owned();
            let enabled = !name.ends_with(".disabled");
            let title = name.trim_end_matches(".disabled").trim_end_matches(".zip").to_string();
            out.push(DataPack {
                file: format!("{wname}/{name}"),
                kind: "datapack",
                enabled,
                title,
                icon: String::new(),
                version: String::new(),
                source: String::new(),
                project: String::new(),
                description: format!("In {wname}"),
                authors: String::new(),
            });
        }
    }
    out.sort_by_key(|d| d.title.to_lowercase());
    Ok(out)
}

fn datapack_path(instance: &str, file: &str) -> Result<PathBuf, String> {
    let (world, pack) = file.split_once('/').ok_or("Unknown data pack.")?;
    Ok(game_dir(instance)?.join("saves").join(safe_file(world)?).join("datapacks").join(safe_file(pack)?))
}

/// Switched off = renamed to .disabled, so the world no longer finds it; switching on renames it back.
#[tauri::command]
pub fn toggle_datapack(instance: String, file: String) -> Result<(), String> {
    let from = datapack_path(&instance, &file)?;
    let name = from.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
    let to = match name.strip_suffix(".disabled") {
        Some(n) => from.with_file_name(n),
        None => from.with_file_name(format!("{name}.disabled")),
    };
    std::fs::rename(&from, &to).map_err(|_| "Couldn't switch it. Close Minecraft first if it's running.".to_string())
}

#[tauri::command(async)]
pub fn remove_datapack(instance: String, file: String) -> Result<(), String> {
    let p = datapack_path(&instance, &file)?;
    (if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) })
        .map_err(|_| "Couldn't remove it. Close Minecraft first if it's running.".to_string())
}

// ---------------------------------------------------------------- files

#[derive(Serialize)]
pub struct Entry {
    name: String,
    dir: bool,
    size: u64,
    modified: u64,
}

fn inside(instance: &str, rel: &str) -> Result<PathBuf, String> {
    let base = game_dir(instance)?;
    Ok(if rel.is_empty() { base } else { base.join(safe_rel(rel)?) })
}

/// One folder of the instance's game files, folders first.
#[tauri::command(async)]
pub fn list_dir(instance: String, rel: String) -> Result<Vec<Entry>, String> {
    let dir = inside(&instance, &rel)?;
    let mut out: Vec<Entry> = std::fs::read_dir(&dir)
        .map_err(|_| "That folder isn't there any more.".to_string())?
        .flatten()
        .map(|x| {
            let meta = x.metadata().ok();
            Entry {
                name: x.file_name().to_string_lossy().into_owned(),
                dir: meta.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                modified: modified(&x.path()),
            }
        })
        .collect();
    out.sort_by(|a, b| b.dir.cmp(&a.dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

#[tauri::command]
pub fn open_dir(instance: String, rel: String) -> Result<(), String> {
    let dir = inside(&instance, &rel)?;
    std::fs::create_dir_all(&dir).map_err(e)?;
    open_path(&dir);
    Ok(())
}

// ---------------------------------------------------------------- worlds

#[derive(Serialize)]
pub struct World {
    id: String,
    icon: String,
    modified: u64,
    packs: usize,
}

#[tauri::command(async)]
pub fn list_worlds(instance: String) -> Result<Vec<World>, String> {
    let saves = game_dir(&instance)?.join("saves");
    let mut out: Vec<World> = std::fs::read_dir(&saves)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|w| w.path().join("level.dat").exists())
        .map(|w| {
            let icon = std::fs::read(w.path().join("icon.png"))
                .ok()
                .filter(|b| b.starts_with(b"\x89PNG"))
                .map(|b| format!("data:image/png;base64,{}", crate::installed::base64(&b)))
                .unwrap_or_default();
            World {
                id: w.file_name().to_string_lossy().into_owned(),
                icon,
                modified: modified(&w.path().join("level.dat")),
                packs: std::fs::read_dir(w.path().join("datapacks")).map(|d| d.count()).unwrap_or(0),
            }
        })
        .collect();
    out.sort_by_key(|w| std::cmp::Reverse(w.modified));
    Ok(out)
}

#[tauri::command]
pub fn open_world(instance: String, id: String) -> Result<(), String> {
    let dir = game_dir(&instance)?.join("saves").join(safe_file(&id)?);
    open_path(&dir);
    Ok(())
}

/// Into the Recycle Bin or the Trash, never gone for good: it's someone's world.
#[tauri::command(async)]
pub fn delete_world(instance: String, id: String) -> Result<(), String> {
    let dir = game_dir(&instance)?.join("saves").join(safe_file(&id)?);
    if !dir.join("level.dat").exists() {
        return Err("That world isn't there any more.".into());
    }
    crate::mc::to_recycle_bin(&dir)
}

// ---------------------------------------------------------------- share: export and import a .mrpack

fn sha1_hex(b: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(b);
    h.finalize().iter().map(|x| format!("{x:02x}")).collect()
}

fn add_folder(zip: &mut zip::ZipWriter<std::fs::File>, opts: zip::write::SimpleFileOptions, root: &Path, dir: &Path, prefix: &str) -> Result<(), String> {
    for x in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = x.path();
        let rel = p.strip_prefix(root).map_err(e)?.to_string_lossy().replace('\\', "/");
        if p.is_dir() {
            add_folder(zip, opts, root, &p, prefix)?;
        } else if let Ok(bytes) = std::fs::read(&p) {
            zip.start_file(format!("{prefix}{rel}"), opts).map_err(e)?;
            zip.write_all(&bytes).map_err(e)?;
        }
    }
    Ok(())
}

/// Saves the instance as a Modrinth-format modpack in Downloads, so a friend can open it in this launcher (or
/// Modrinth) and get the same game. Mods Modrinth knows are listed by download link; anything else (like the
/// Anti-Horizon client) goes inside the file. Switched-off files stay out. Returns where it was saved.
#[tauri::command]
pub async fn export_mrpack(instance: String, with_config: bool) -> Result<String, String> {
    let inst = load_instance(&instance)?;
    let game = game_dir(&instance)?;
    let mut files: Vec<(String, Vec<u8>, String)> = Vec::new();
    for folder in ["mods", "resourcepacks", "shaderpacks"] {
        for x in std::fs::read_dir(game.join(folder)).into_iter().flatten().flatten() {
            let name = x.file_name().to_string_lossy().into_owned();
            if name.ends_with(".disabled") || !x.path().is_file() {
                continue;
            }
            if let Ok(bytes) = std::fs::read(x.path()) {
                let sha = sha1_hex(&bytes);
                files.push((format!("{folder}/{name}"), bytes, sha));
            }
        }
    }
    let hashes: Vec<&str> = files.iter().map(|f| f.2.as_str()).collect();
    let known: Value = if hashes.is_empty() {
        json!({})
    } else {
        client()
            .post("https://api.modrinth.com/v2/version_files")
            .json(&json!({ "hashes": hashes, "algorithm": "sha1" }))
            .send()
            .await
            .map_err(|err| format!("Couldn't reach Modrinth ({err})"))?
            .json()
            .await
            .map_err(e)?
    };

    let name = if inst.name.trim().is_empty() { inst.id.clone() } else { inst.name.clone() };
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
    let downloads = crate::platform::downloads();
    std::fs::create_dir_all(&downloads).map_err(e)?;
    let mut out = downloads.join(format!("{}.mrpack", safe.trim()));
    let mut n = 2;
    while out.exists() {
        out = downloads.join(format!("{} ({n}).mrpack", safe.trim()));
        n += 1;
    }

    let file = std::fs::File::create(&out).map_err(e)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut listed = Vec::new();
    for (path, bytes, sha) in &files {
        let hit = known.get(sha).and_then(|v| v.get("files")).and_then(Value::as_array).and_then(|fs| {
            fs.iter().find(|f| f.pointer("/hashes/sha1").and_then(Value::as_str) == Some(sha.as_str()))
        });
        match hit {
            Some(f) => listed.push(json!({
                "path": path,
                "hashes": { "sha1": sha, "sha512": f.pointer("/hashes/sha512").cloned().unwrap_or(Value::Null) },
                "downloads": [f.get("url").cloned().unwrap_or(Value::Null)],
                "fileSize": bytes.len(),
            })),
            None => {
                zip.start_file(format!("overrides/{path}"), opts).map_err(e)?;
                zip.write_all(bytes).map_err(e)?;
            }
        }
    }
    if with_config {
        add_folder(&mut zip, opts, &game, &game.join("config"), "overrides/")?;
        for f in ["options.txt", "servers.dat"] {
            if let Ok(bytes) = std::fs::read(game.join(f)) {
                zip.start_file(format!("overrides/{f}"), opts).map_err(e)?;
                zip.write_all(&bytes).map_err(e)?;
            }
        }
    }
    let loader_key = match inst.loader.as_str() {
        "fabric" => "fabric-loader",
        "quilt" => "quilt-loader",
        "forge" => "forge",
        "neoforge" => "neoforge",
        _ => "",
    };
    let mut deps: HashMap<&str, String> = HashMap::from([("minecraft", inst.version.clone())]);
    if !loader_key.is_empty() {
        deps.insert(loader_key, inst.loader_version.clone());
    }
    let index = json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": "1.0.0",
        "name": name,
        "summary": "Shared from the Anti Horizon Launcher",
        "files": listed,
        "dependencies": deps,
    });
    zip.start_file("modrinth.index.json", opts).map_err(e)?;
    zip.write_all(serde_json::to_string_pretty(&index).map_err(e)?.as_bytes()).map_err(e)?;
    zip.finish().map_err(e)?;

    crate::platform::reveal(&out);
    Ok(out.display().to_string())
}

/// A .mrpack a friend shared, sent by the window as raw bytes: it becomes a new instance, like a Modrinth modpack.
#[tauri::command]
pub async fn import_mrpack(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<crate::mods::PackResult, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("No file came through.".into());
    };
    let dir = crate::store::root().join("cache").join("modpacks");
    std::fs::create_dir_all(&dir).map_err(e)?;
    let path = dir.join(format!("import-{}.mrpack", crate::mc::now()));
    std::fs::write(&path, bytes).map_err(e)?;
    let result = crate::mods::install_local_pack(&app, &path, String::new()).await;
    let _ = std::fs::remove_file(&path);
    result
}
