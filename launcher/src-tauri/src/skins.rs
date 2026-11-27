// The skin changer: the selected account's skin and capes, through Minecraft's own profile service.
use crate::auth::account_for_launch;
use crate::net::client;
use crate::store::e;
use serde::Serialize;
use serde_json::{json, Value};

const PROFILE: &str = "https://api.minecraftservices.com/minecraft/profile";

#[derive(Serialize)]
pub struct Cape {
    id: String,
    alias: String,
    active: bool,
}

#[derive(Serialize)]
pub struct Profile {
    name: String,
    /// "classic" (4-pixel arms) or "slim" (3-pixel arms).
    variant: String,
    /// The skin picture as a data: URL, for the preview.
    skin: String,
    capes: Vec<Cape>,
}

async fn picture(url: &str) -> String {
    // Mojang hands out http:// texture links; the same files are on https.
    let url = url.replacen("http://", "https://", 1);
    match client().get(&url).send().await {
        Ok(r) => match r.bytes().await {
            Ok(b) if b.starts_with(b"\x89PNG") => format!("data:image/png;base64,{}", crate::installed::base64(&b)),
            _ => String::new(),
        },
        Err(_) => String::new(),
    }
}

async fn check(r: reqwest::Response) -> Result<Value, String> {
    let status = r.status();
    let v: Value = r.json().await.unwrap_or(Value::Null);
    if status.is_success() {
        return Ok(v);
    }
    let why = v.get("errorMessage").and_then(Value::as_str).unwrap_or("").to_string();
    Err(match status.as_u16() {
        401 => "Minecraft didn't accept the sign-in. Sign out of the account and add it again.".into(),
        429 => "Minecraft is limiting skin changes right now. Try again in a minute.".into(),
        _ if !why.is_empty() => why,
        s => format!("Minecraft's skin service said no (error {s})."),
    })
}

async fn read_profile(token: &str) -> Result<Profile, String> {
    let v = check(client().get(PROFILE).bearer_auth(token).send().await.map_err(|err| format!("Couldn't reach Minecraft ({err})"))?).await?;
    let active = v
        .get("skins")
        .and_then(Value::as_array)
        .and_then(|s| s.iter().find(|x| x.get("state").and_then(Value::as_str) == Some("ACTIVE")).cloned());
    let (variant, skin) = match active {
        Some(s) => (
            s.get("variant").and_then(Value::as_str).unwrap_or("CLASSIC").to_lowercase(),
            picture(s.get("url").and_then(Value::as_str).unwrap_or_default()).await,
        ),
        None => ("classic".into(), String::new()),
    };
    let capes = v
        .get("capes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|c| Cape {
            id: c.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
            alias: c.get("alias").and_then(Value::as_str).unwrap_or("Cape").to_string(),
            active: c.get("state").and_then(Value::as_str) == Some("ACTIVE"),
        })
        .collect();
    Ok(Profile { name: v.get("name").and_then(Value::as_str).unwrap_or_default().to_string(), variant, skin, capes })
}

/// The selected account's skin and capes.
#[tauri::command]
pub async fn get_profile() -> Result<Profile, String> {
    let acct = account_for_launch().await?;
    read_profile(&acct.mc_token).await
}

/// A new skin: the window sends the PNG as raw bytes, with the arm model in the "x-variant" header.
#[tauri::command]
pub async fn upload_skin(request: tauri::ipc::Request<'_>) -> Result<Profile, String> {
    let tauri::ipc::InvokeBody::Raw(png) = request.body() else {
        return Err("No picture came through.".into());
    };
    let variant = request.headers().get("x-variant").and_then(|v| v.to_str().ok()).unwrap_or("classic");
    let variant = if variant == "slim" { "slim" } else { "classic" };
    // A Minecraft skin is a PNG, 64 by 64 (or the old 64 by 32).
    let size = (png.len() > 24 && png.starts_with(b"\x89PNG")).then(|| {
        (u32::from_be_bytes([png[16], png[17], png[18], png[19]]), u32::from_be_bytes([png[20], png[21], png[22], png[23]]))
    });
    match size {
        Some((64, 64)) | Some((64, 32)) => {}
        Some((w, h)) => return Err(format!("That picture is {w} by {h}. A skin is 64 by 64 pixels.")),
        None => return Err("That isn't a PNG picture.".into()),
    }
    let acct = account_for_launch().await?;
    // Built by hand: the one multipart request the launcher makes.
    let boundary = format!("antihorizon{}", crate::mc::now());
    let mut body = Vec::new();
    body.extend(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"variant\"\r\n\r\n{variant}\r\n").as_bytes());
    body.extend(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"skin.png\"\r\nContent-Type: image/png\r\n\r\n").as_bytes());
    body.extend_from_slice(png);
    body.extend(format!("\r\n--{boundary}--\r\n").as_bytes());
    let r = client()
        .post(format!("{PROFILE}/skins"))
        .bearer_auth(&acct.mc_token)
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
        .body(body)
        .send()
        .await
        .map_err(|err| format!("Couldn't reach Minecraft ({err})"))?;
    check(r).await?;
    read_profile(&acct.mc_token).await
}

/// Back to the default skin.
#[tauri::command]
pub async fn reset_skin() -> Result<Profile, String> {
    let acct = account_for_launch().await?;
    let r = client().delete(format!("{PROFILE}/skins/active")).bearer_auth(&acct.mc_token).send().await.map_err(e)?;
    check(r).await?;
    read_profile(&acct.mc_token).await
}

/// Wears one of the account's capes, or none (empty id).
#[tauri::command]
pub async fn set_cape(id: String) -> Result<Profile, String> {
    let acct = account_for_launch().await?;
    let url = format!("{PROFILE}/capes/active");
    let r = if id.is_empty() {
        client().delete(&url).bearer_auth(&acct.mc_token).send().await
    } else {
        client().put(&url).bearer_auth(&acct.mc_token).json(&json!({ "capeId": id })).send().await
    }
    .map_err(e)?;
    check(r).await?;
    read_profile(&acct.mc_token).await
}

// ---------------------------------------------------------------- the skin library: default skins and skin packs

#[derive(Serialize)]
pub struct LibSkin {
    id: String,
    name: String,
    /// "classic" or "slim" for the default skins; empty when it isn't known (the window works it out).
    variant: String,
    data: String,
}

#[derive(Serialize)]
pub struct Pack {
    name: String,
    skins: Vec<LibSkin>,
}

const DEFAULTS: [&str; 9] = ["steve", "alex", "ari", "efe", "kai", "makena", "noor", "sunny", "zuri"];

fn png_url(b: &[u8]) -> String {
    format!("data:image/png;base64,{}", crate::installed::base64(b))
}

/// Minecraft's nine default characters in both arm styles, read from a game jar already on this PC (so none of
/// Mojang's pictures ship inside the launcher). Empty until some instance has downloaded Minecraft.
#[tauri::command(async)]
pub fn default_skins() -> Vec<LibSkin> {
    let mut jars: Vec<std::path::PathBuf> = std::fs::read_dir(crate::store::root().join("versions"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|d| d.path().join(format!("{}.jar", d.file_name().to_string_lossy())))
        .filter(|p| p.is_file())
        .collect();
    jars.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
    for jar in jars {
        let Ok(file) = std::fs::File::open(&jar) else { continue };
        let Ok(mut zip) = zip::ZipArchive::new(file) else { continue };
        let mut out = Vec::new();
        for (style, variant) in [("wide", "classic"), ("slim", "slim")] {
            for name in DEFAULTS {
                let Ok(mut f) = zip.by_name(&format!("assets/minecraft/textures/entity/player/{style}/{name}.png")) else { continue };
                let mut b = Vec::new();
                if std::io::Read::read_to_end(&mut f, &mut b).is_ok() {
                    let mut title = name.to_string();
                    title[..1].make_ascii_uppercase();
                    out.push(LibSkin { id: format!("{style}/{name}"), name: title, variant: variant.into(), data: png_url(&b) });
                }
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    Vec::new()
}

fn library() -> std::path::PathBuf {
    crate::store::root().join("skins")
}

fn clean(s: &str) -> String {
    let c: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' }).collect();
    let c = c.trim().chars().take(48).collect::<String>();
    if c.is_empty() {
        "Skin".into()
    } else {
        c
    }
}

fn is_skin(b: &[u8]) -> bool {
    b.len() > 24
        && b.starts_with(b"\x89PNG")
        && u32::from_be_bytes([b[16], b[17], b[18], b[19]]) == 64
        && matches!(u32::from_be_bytes([b[20], b[21], b[22], b[23]]), 64 | 32)
}

/// The player's skin packs (folders under the launcher's skins folder), "My skins" first.
#[tauri::command(async)]
pub fn skin_packs() -> Vec<Pack> {
    let mut packs: Vec<Pack> = std::fs::read_dir(library())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|d| d.path().is_dir())
        .map(|d| {
            let mut skins: Vec<LibSkin> = std::fs::read_dir(d.path())
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|f| {
                    let b = std::fs::read(f.path()).ok().filter(|b| is_skin(b))?;
                    let id = f.file_name().to_string_lossy().into_owned();
                    Some(LibSkin { name: id.trim_end_matches(".png").to_string(), id, variant: String::new(), data: png_url(&b) })
                })
                .collect();
            skins.sort_by_key(|s| s.name.to_lowercase());
            Pack { name: d.file_name().to_string_lossy().into_owned(), skins }
        })
        .collect();
    packs.sort_by_key(|p| (p.name != "My skins", p.name.to_lowercase()));
    packs
}

/// Keeps a skin in a pack ("x-pack" and "x-name" headers, the PNG as the raw body).
#[tauri::command]
pub fn add_to_library(request: tauri::ipc::Request<'_>) -> Result<Vec<Pack>, String> {
    let tauri::ipc::InvokeBody::Raw(png) = request.body() else {
        return Err("No picture came through.".into());
    };
    if !is_skin(png) {
        return Err("That isn't a 64 by 64 Minecraft skin.".into());
    }
    let header = |k: &str| request.headers().get(k).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let dir = library().join(clean(&header("x-pack")));
    std::fs::create_dir_all(&dir).map_err(e)?;
    let base = clean(&header("x-name"));
    let mut path = dir.join(format!("{base}.png"));
    let mut n = 2;
    while path.exists() {
        if std::fs::read(&path).map(|b| b == *png).unwrap_or(false) {
            return Ok(skin_packs()); // already there
        }
        path = dir.join(format!("{base} {n}.png"));
        n += 1;
    }
    std::fs::write(&path, png).map_err(e)?;
    Ok(skin_packs())
}

/// A .zip of skins becomes a pack named after the file ("x-name"); anything that isn't a skin is skipped.
#[tauri::command]
pub fn import_skin_pack(request: tauri::ipc::Request<'_>) -> Result<Vec<Pack>, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("No file came through.".into());
    };
    let name = clean(request.headers().get("x-name").and_then(|v| v.to_str().ok()).unwrap_or("Skin pack"));
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.clone())).map_err(|_| "That isn't a .zip file.".to_string())?;
    let dir = library().join(&name);
    std::fs::create_dir_all(&dir).map_err(e)?;
    let mut added = 0;
    for i in 0..zip.len() {
        let Ok(mut f) = zip.by_index(i) else { continue };
        let fname = f.name().rsplit('/').next().unwrap_or("").to_string();
        if !fname.to_ascii_lowercase().ends_with(".png") {
            continue;
        }
        let mut b = Vec::new();
        if std::io::Read::read_to_end(&mut f, &mut b).is_ok() && is_skin(&b) {
            let stem = &fname[..fname.len() - 4];
            std::fs::write(dir.join(format!("{}.png", clean(stem))), &b).map_err(e)?;
            added += 1;
        }
    }
    if added == 0 {
        let _ = std::fs::remove_dir(&dir);
        return Err("No Minecraft skins (64 by 64 PNGs) in that file.".into());
    }
    Ok(skin_packs())
}

#[tauri::command]
pub fn delete_library_skin(pack: String, id: String) -> Result<Vec<Pack>, String> {
    let dir = library().join(clean(&pack));
    let _ = std::fs::remove_file(dir.join(crate::mods::safe_file(&id)?));
    if std::fs::read_dir(&dir).map(|mut d| d.next().is_none()).unwrap_or(false) {
        let _ = std::fs::remove_dir(&dir);
    }
    Ok(skin_packs())
}

/// A whole pack goes to the Recycle Bin.
#[tauri::command]
pub fn delete_skin_pack(pack: String) -> Result<Vec<Pack>, String> {
    let dir = library().join(clean(&pack));
    if dir.is_dir() {
        crate::mc::to_recycle_bin(&dir)?;
    }
    Ok(skin_packs())
}
