// Launcher updates. When it opens, the launcher asks anti-horizon.com whether a newer version exists. If one does, it
// downloads that version's installer (the same NSIS setup people install with), checks its signature against the
// public key built into the launcher, and runs it in passive mode: a progress bar, no questions, then the launcher
// opens again on the new version. Instances, worlds, mods and settings are untouched.
use crate::store::e;
use serde::Serialize;
use serde_json::json;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};

/// A downloaded, verified update waiting to be installed.
static READY: Mutex<Option<(Update, Vec<u8>)>> = Mutex::new(None);

#[derive(Serialize, Clone)]
pub struct Found {
    version: String,
    notes: String,
}

fn found(u: &Update) -> Found {
    Found { version: u.version.clone(), notes: u.body.clone().unwrap_or_default() }
}

/// Only an installed launcher updates itself (setup puts uninstall.exe beside it). A build run straight from the
/// project, like the one used while developing, never does.
fn installed() -> bool {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("uninstall.exe").is_file())).unwrap_or(false)
}

fn updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    let mut builder = app.updater_builder();
    // Tests point this at a local copy of the update files instead of the live site. Whatever it points at, an
    // installer still has to carry a valid signature from the launcher's own key.
    if let Ok(url) = std::env::var("AH_LAUNCHER_UPDATES") {
        builder = builder.endpoints(vec![url.parse().map_err(e)?]).map_err(e)?;
    }
    builder.build().map_err(e)
}

/// Looks for a newer launcher and, if there is one, downloads it. Returns its version once it is ready to install,
/// or nothing when this launcher is already the newest.
#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<Option<Found>, String> {
    if let Some((u, _)) = READY.lock().unwrap().as_ref() {
        return Ok(Some(found(u)));
    }
    if !installed() {
        return Ok(None);
    }
    let checked = updater(&app)?.check().await.map_err(|err| format!("Couldn't check for launcher updates: {err}"))?;
    let Some(update) = checked else { return Ok(None) };
    let _ = app.emit("update", json!({ "version": update.version, "done": 0, "total": 0 }));
    let mut done: u64 = 0;
    let mut shown = 0u64;
    let bytes = update
        .download(
            |chunk, total| {
                done += chunk as u64;
                // A few events per download, not one per chunk.
                let pct = total.map(|t| done * 100 / t.max(1)).unwrap_or(0);
                if pct != shown {
                    shown = pct;
                    let _ = app.emit("update", json!({ "version": update.version, "done": done, "total": total }));
                }
            },
            || {},
        )
        .await
        .map_err(|err| format!("The launcher update didn't download: {err}"))?;
    let f = found(&update);
    *READY.lock().unwrap() = Some((update, bytes));
    Ok(Some(f))
}

/// Runs the downloaded installer in passive mode. The launcher closes for it and opens again when it is done.
#[tauri::command]
pub fn update_install() -> Result<(), String> {
    let (update, bytes) = READY.lock().unwrap().take().ok_or("No launcher update is ready.")?;
    update.install(bytes).map_err(|err| format!("The launcher update didn't start: {err}"))
}
