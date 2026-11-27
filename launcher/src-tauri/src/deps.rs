// Mods that need other mods: the game refuses to start ("Incompatible mods found") when one is missing, and not every
// author lists what their mod needs on Modrinth. So this reads what each mod itself says it needs (its fabric.mod.json
// or quilt.mod.json, exactly what the game checks), and installs whatever isn't there, from Modrinth. It runs after
// installing mods and before every Play of a Fabric or Quilt instance.
use crate::mc::{game_dir, load_instance};
use crate::mods::{install_mod, mr_get};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;

/// Provided by the game and the loader themselves, never something to download.
const BUILT_IN: [&str; 8] = ["minecraft", "java", "fabricloader", "fabric-loader", "quilt_loader", "mixinextras", "quilt_base", "quilted_fabric_api"];

/// Mod ids whose Modrinth project has a different name.
const KNOWN: [(&str, &str); 18] = [
    ("fabric", "fabric-api"),
    ("fabric-api", "fabric-api"),
    ("fabric-language-kotlin", "fabric-language-kotlin"),
    ("cloth-config", "cloth-config"),
    ("cloth-config2", "cloth-config"),
    ("yet_another_config_lib_v3", "yacl"),
    ("yet-another-config-lib", "yacl"),
    ("owo", "owo-lib"),
    ("architectury", "architectury-api"),
    ("forgeconfigapiport", "forge-config-api-port"),
    ("puzzleslib", "puzzles-lib"),
    ("resourcefullib", "resourceful-lib"),
    ("modmenu", "modmenu"),
    ("geckolib", "geckolib"),
    ("playeranimator", "playeranimator"),
    ("libz", "libz"),
    ("cardinal-components", "cardinal-components-api"),
    ("lithostitched", "lithostitched"),
];

struct ModInfo {
    provides: Vec<String>,
    depends: Vec<String>,
}

/// What one mod file says: the ids it gives (its own, its "provides", and the mods bundled inside it) and the ids it
/// needs. Bundled jars are looked into one level down, which is where Fabric API keeps its modules.
fn read_mod(bytes: &[u8], depth: u8) -> Option<ModInfo> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut text = String::new();
    let quilt = if zip.index_for_name("fabric.mod.json").is_some() {
        false
    } else if zip.index_for_name("quilt.mod.json").is_some() {
        true
    } else {
        return None;
    };
    zip.by_name(if quilt { "quilt.mod.json" } else { "fabric.mod.json" }).ok()?.read_to_string(&mut text).ok()?;
    // Some mods have raw control characters in their descriptions; drop them so the JSON still reads.
    let clean: String = text.chars().filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t')).collect();
    let v: Value = serde_json::from_str(&clean).ok()?;
    let mut info = ModInfo { provides: Vec::new(), depends: Vec::new() };
    let strings = |x: Option<&Value>| -> Vec<String> {
        x.and_then(Value::as_array).into_iter().flatten().filter_map(|p| p.as_str().or_else(|| p.get("id").and_then(Value::as_str))).map(String::from).collect()
    };
    if quilt {
        let q = v.get("quilt_loader")?;
        info.provides.push(q.get("id").and_then(Value::as_str)?.to_string());
        info.provides.extend(strings(q.get("provides")));
        for d in q.get("depends").and_then(Value::as_array).into_iter().flatten() {
            let optional = d.get("optional").and_then(Value::as_bool).unwrap_or(false);
            match d.as_str().or_else(|| d.get("id").and_then(Value::as_str)) {
                Some(id) if !optional => info.depends.push(id.to_string()),
                _ => {}
            }
        }
    } else {
        info.provides.push(v.get("id").and_then(Value::as_str)?.to_string());
        info.provides.extend(strings(v.get("provides")));
        if let Some(d) = v.get("depends").and_then(Value::as_object) {
            info.depends.extend(d.keys().cloned());
        }
    }
    if depth == 0 {
        let names: Vec<String> = zip.file_names().filter(|n| n.starts_with("META-INF/jars/") && n.ends_with(".jar")).map(String::from).collect();
        for n in names {
            let mut inner = Vec::new();
            if let Ok(mut f) = zip.by_name(&n) {
                if f.read_to_end(&mut inner).is_ok() {
                    if let Some(sub) = read_mod(&inner, 1) {
                        info.provides.extend(sub.provides);
                    }
                }
            }
        }
    }
    Some(info)
}

/// Every mod id the instance's switched-on mods need that none of them provides.
fn missing(game: &Path) -> Vec<String> {
    let mut provided: HashSet<String> = BUILT_IN.iter().map(|s| s.to_string()).collect();
    let mut needed: Vec<String> = Vec::new();
    for e in std::fs::read_dir(game.join("mods")).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        if !name.ends_with(".jar") {
            continue;
        }
        let Ok(bytes) = std::fs::read(e.path()) else { continue };
        if let Some(info) = read_mod(&bytes, 0) {
            provided.extend(info.provides);
            needed.extend(info.depends);
        }
    }
    let mut out: Vec<String> = needed.into_iter().filter(|id| !provided.contains(id)).collect();
    out.sort();
    out.dedup();
    out
}

/// The Modrinth project for a mod id: a known name, any Fabric API module, or a project with that id as its name.
async fn project_for(id: &str) -> Option<String> {
    if let Some((_, p)) = KNOWN.iter().find(|(k, _)| *k == id) {
        return Some(p.to_string());
    }
    if id.starts_with("fabric-") && (id.ends_with("-v0") || id.ends_with("-v1") || id.ends_with("-v2") || id.ends_with("-v3") || id.ends_with("-api") || id.contains("-api-")) {
        return Some("fabric-api".into());
    }
    if id.starts_with("cardinal-components") {
        return Some("cardinal-components-api".into());
    }
    for slug in [id.to_string(), id.replace('_', "-")] {
        if let Ok(v) = mr_get(&format!("https://api.modrinth.com/v2/project/{slug}"), &[]).await {
            if v.get("project_type").and_then(Value::as_str) == Some("mod") {
                return v.get("id").and_then(Value::as_str).map(String::from);
            }
        }
    }
    None
}

#[derive(Serialize, Default)]
pub struct Fixed {
    /// Files added to make the instance's mods complete.
    pub added: Vec<String>,
    /// Mod ids nothing on Modrinth could be found for.
    pub unknown: Vec<String>,
}

/// Installs what the instance's mods need and don't have. Fabric and Quilt instances only.
pub async fn fix(instance: &str) -> Result<Fixed, String> {
    let inst = load_instance(instance)?;
    let mut out = Fixed::default();
    if inst.loader != "fabric" && inst.loader != "quilt" {
        return Ok(out);
    }
    let game = game_dir(instance)?;
    let mut tried: HashMap<String, bool> = HashMap::new();
    for _ in 0..4 {
        let need: Vec<String> = missing(&game).into_iter().filter(|id| !tried.contains_key(id)).collect();
        if need.is_empty() {
            break;
        }
        for id in need {
            let ok = match project_for(&id).await {
                Some(project) => match install_mod(instance.to_string(), "modrinth".into(), project, "mod".into(), String::new(), String::new()).await {
                    Ok(files) => {
                        out.added.extend(files);
                        true
                    }
                    Err(_) => false,
                },
                None => false,
            };
            if !ok {
                out.unknown.push(id.clone());
            }
            tried.insert(id, ok);
        }
    }
    out.added.sort();
    out.added.dedup();
    Ok(out)
}

#[tauri::command]
pub async fn fix_dependencies(instance: String) -> Result<Fixed, String> {
    fix(&instance).await
}

#[cfg(test)]
mod tests {
    use super::read_mod;
    use std::io::Write;

    fn jar(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            for (name, data) in files {
                z.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
                z.write_all(data).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    /// Lists what a real instance would be missing, without installing anything: AH_GAME=<its minecraft folder>.
    #[test]
    #[ignore]
    fn real_instance() {
        let game = std::env::var("AH_GAME").expect("set AH_GAME");
        println!("missing: {:?}", super::missing(std::path::Path::new(&game)));
    }

    #[test]
    fn reads_ids_needs_and_bundled_mods() {
        let inner = jar(&[("fabric.mod.json", br#"{"id":"fabric-api-base","version":"1"}"#)]);
        let outer = jar(&[
            ("fabric.mod.json", br#"{"id":"fabric-api","provides":["fabric"],"depends":{"fabricloader":"*","minecraft":"1.21.11"}}"#),
            ("META-INF/jars/base.jar", &inner),
        ]);
        let info = read_mod(&outer, 0).unwrap();
        assert!(info.provides.contains(&"fabric-api".to_string()));
        assert!(info.provides.contains(&"fabric".to_string()));
        assert!(info.provides.contains(&"fabric-api-base".to_string()));
        assert_eq!(info.depends.len(), 2);
    }
}
