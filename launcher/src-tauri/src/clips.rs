// Live backgrounds: 20-second loops of Minecraft recordings, played behind the window (and on live capes in the
// Cosmetics preview). They're too big to ship inside the launcher, so each comes from anti-horizon.com the first time
// it's needed, is checked against the size and SHA-256 listed in dist/clips.json (built into the launcher), and is kept
// in the launcher's cache folder after that.
use crate::net::client;
use crate::store::{e, root};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::ipc::Response;

const BASE: &str = "https://anti-horizon.com/launcher/clips/";
const LIST: &str = include_str!("../../dist/clips.json");

#[derive(Deserialize)]
struct Clip {
    id: String,
    file: String,
    size: u64,
    sha256: String,
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// The clip's video, from the cache or downloaded once. The window turns the bytes into a video it can play.
#[tauri::command]
pub async fn clip(id: String) -> Result<Response, String> {
    let list: Vec<Clip> = serde_json::from_str(LIST).map_err(e)?;
    let c = list.into_iter().find(|c| c.id == id).ok_or("There's no clip by that name.")?;
    let dir = root().join("cache").join("clips");
    let path = dir.join(&c.file);
    let fits = |b: &[u8]| b.len() as u64 == c.size && sha256(b) == c.sha256;
    if let Ok(bytes) = tauri::async_runtime::spawn_blocking({
        let path = path.clone();
        move || std::fs::read(path)
    })
    .await
    .map_err(e)?
    {
        if fits(&bytes) {
            return Ok(Response::new(bytes));
        }
    }
    let bytes = client()
        .get(format!("{BASE}{}", c.file))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|err| format!("Couldn't download the clip ({err})"))?
        .bytes()
        .await
        .map_err(e)?
        .to_vec();
    if !fits(&bytes) {
        return Err("The clip didn't download properly.".into());
    }
    std::fs::create_dir_all(&dir).map_err(e)?;
    let part = dir.join(format!("{}.part", c.file));
    std::fs::write(&part, &bytes).map_err(e)?;
    std::fs::rename(&part, &path).map_err(e)?;
    Ok(Response::new(bytes))
}
