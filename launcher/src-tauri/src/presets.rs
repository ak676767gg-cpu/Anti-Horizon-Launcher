// The Anti-Horizon client in every instance it can run in: it ships inside the launcher, goes into new Fabric instances
// of the versions it supports, and is added or brought up to date before each Play.
use crate::mc::{game_dir, progress, Instance};
use crate::mods::{install_mod, ledger_path, load_ledger, Entry};
use crate::store::{e, write_json};
use std::path::Path;
use tauri::AppHandle;

/// The Anti-Horizon client mod, built into the launcher, so a preset always gets the client this launcher came with.
/// release.ps1 copies the newest build of the client project here before each release.
const CLIENT_JAR: &[u8] = include_bytes!("../presets/anti-horizon-client.jar");
const CLIENT_FILE: &str = "anti-horizon-client.jar";
const CLIENT_VERSION: &str = "0.1.0";

fn put_client(id: &str) -> Result<(), String> {
    let dir = game_dir(id)?.join("mods");
    std::fs::create_dir_all(&dir).map_err(e)?;
    std::fs::write(dir.join(CLIENT_FILE), CLIENT_JAR).map_err(e)?;
    let mut ledger = load_ledger(id);
    ledger.insert(
        CLIENT_FILE.to_string(),
        Entry {
            source: "anti-horizon".into(),
            project: "client".into(),
            title: "Anti Horizon Client".into(),
            icon: String::new(),
            version: CLIENT_VERSION.into(),
        },
    );
    write_json(&ledger_path(id)?, &ledger)
}

/// The Minecraft versions the Anti-Horizon client jar has a build for (one jar holds them all).
fn client_supports(mc: &str) -> bool {
    mc == "1.21.11" || ["26.1", "26.2", "26.3"].iter().any(|v| mc == *v || mc.starts_with(&format!("{v}.")))
}

/// A new instance comes with the Anti-Horizon client already in its mods, when it's Fabric on a version the client
/// supports (and "client in every instance" is on). Fabric API, which the client needs, is added at its first Play.
pub fn preinstall(inst: &Instance) {
    if inst.loader == "fabric" && client_supports(&inst.version) && inst.dev.is_empty() && crate::store::load_settings().client_everywhere {
        let _ = put_client(&inst.id);
    }
}

/// Before a Fabric instance of a supported version (1.21.11, 26.1 to 26.3) starts: it gets the Anti-Horizon client (and the Fabric API the client needs), and
/// an older client is swapped for the one in this launcher, so a launcher update also updates the client. With
/// "client in every instance" off in Settings, only instances made from a preset get it. A client the player switched
/// off, or a copy of their own (a development build), is left alone.
pub async fn ensure_client(app: &AppHandle, inst: &Instance, game: &Path, everywhere: bool) {
    if inst.loader != "fabric" || !client_supports(&inst.version) || !inst.dev.is_empty() {
        return;
    }
    if inst.preset.is_empty() && !everywhere {
        return;
    }
    let mods = game.join("mods");
    let names: Vec<String> = std::fs::read_dir(&mods)
        .into_iter()
        .flatten()
        .flatten()
        .map(|x| x.file_name().to_string_lossy().to_ascii_lowercase())
        .collect();
    let switched_off = names.iter().any(|n| n == &format!("{CLIENT_FILE}.disabled"));
    let own_copy = names.iter().any(|n| n.starts_with("anti-horizon-client") && n.as_str() != CLIENT_FILE && !n.ends_with(".disabled"));
    if switched_off || own_copy {
        return;
    }
    if !names.iter().any(|n| n.starts_with("fabric-api") && n.ends_with(".jar")) {
        progress(app, &inst.id, "Adding Fabric API", 0, 0);
        if install_mod(inst.id.clone(), "modrinth".into(), "P7dR8mSH".into(), "mod".into(), String::new(), String::new()).await.is_err() {
            return; // offline: the game still starts, just without the client this time
        }
    }
    match std::fs::read(mods.join(CLIENT_FILE)) {
        Ok(have) if have == CLIENT_JAR => {}
        _ => {
            let _ = put_client(&inst.id);
        }
    }
}
