// Microsoft sign-in for Minecraft: a device code (sign in from any browser), then Xbox Live, XSTS and Minecraft's
// own services. Tokens are stored encrypted with Windows DPAPI, so only this Windows user can read them.
use crate::net::client;
use crate::store::{e, root, text};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const DEVICE_CODE: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const SCOPE: &str = "XboxLive.signin offline_access";
const NOT_APPROVED: &str =
    "Minecraft turned this app ID away. New Azure apps need Mojang's approval first: apply at aka.ms/mce-reviewappid.";

/// Bumped to stop a sign-in that is still waiting on the browser.
static LOGIN: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Account {
    pub uuid: String,
    pub name: String,
    pub xuid: String,
    pub refresh_token: String,
    pub mc_token: String,
    pub mc_expires: u64,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Book {
    selected: String,
    accounts: Vec<Account>,
}

/// What the window may see: never a token.
#[derive(Serialize)]
pub struct AccountView {
    uuid: String,
    name: String,
    selected: bool,
}

#[cfg(windows)]
mod sealed {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};

    const UI_FORBIDDEN: u32 = 1;

    fn run(data: &[u8], seal: bool) -> Option<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        // SAFETY: the input blob points at live memory for the call; Windows allocates the output, freed below.
        unsafe {
            let ok = if seal {
                CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), UI_FORBIDDEN, &mut out)
            } else {
                CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), UI_FORBIDDEN, &mut out)
            };
            if ok == 0 || out.pbData.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData as _);
            Some(bytes)
        }
    }

    pub fn seal(data: &[u8]) -> Option<Vec<u8>> {
        run(data, true)
    }

    pub fn open(data: &[u8]) -> Option<Vec<u8>> {
        run(data, false)
    }
}

#[cfg(not(windows))]
mod sealed {
    pub fn seal(data: &[u8]) -> Option<Vec<u8>> {
        Some(data.to_vec())
    }
    pub fn open(data: &[u8]) -> Option<Vec<u8>> {
        Some(data.to_vec())
    }
}

fn book_path() -> PathBuf {
    root().join("accounts.dat")
}

fn load() -> Book {
    std::fs::read(book_path())
        .ok()
        .and_then(|raw| sealed::open(&raw))
        .and_then(|plain| serde_json::from_slice(&plain).ok())
        .unwrap_or_default()
}

fn save(book: &Book) -> Result<(), String> {
    let plain = serde_json::to_vec(book).map_err(e)?;
    let sealed = sealed::seal(&plain).ok_or("Windows couldn't encrypt the account file.")?;
    let tmp = book_path().with_extension("tmp");
    std::fs::write(&tmp, sealed).map_err(e)?;
    std::fs::rename(&tmp, book_path()).map_err(e)
}

fn views(book: &Book) -> Vec<AccountView> {
    book.accounts
        .iter()
        .map(|a| AccountView { uuid: a.uuid.clone(), name: a.name.clone(), selected: a.uuid == book.selected })
        .collect()
}

/// The launcher's own Azure app ID, approved by Mojang for Minecraft sign-in. It isn't in the source: release.ps1 builds
/// it in from a file kept outside the repo, so copies built from the public code can't sign in as this launcher.
pub const CLIENT_ID: &str = match option_env!("AH_MSA_CLIENT_ID") {
    Some(id) => id,
    None => "",
};

fn client_id() -> Result<String, String> {
    if CLIENT_ID.is_empty() {
        return Err("This copy of the launcher was built without its Microsoft sign-in ID, so it can't sign in. Get the launcher from anti-horizon.com.".into());
    }
    Ok(CLIENT_ID.to_string())
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or(s).trim().to_string()
}

#[tauri::command]
pub fn list_accounts() -> Vec<AccountView> {
    views(&load())
}

#[tauri::command]
pub fn select_account(uuid: String) -> Result<Vec<AccountView>, String> {
    let mut book = load();
    if !book.accounts.iter().any(|a| a.uuid == uuid) {
        return Err("That account is gone.".into());
    }
    book.selected = uuid;
    save(&book)?;
    Ok(views(&book))
}

#[tauri::command]
pub fn remove_account(uuid: String) -> Result<Vec<AccountView>, String> {
    let mut book = load();
    book.accounts.retain(|a| a.uuid != uuid);
    if book.selected == uuid {
        book.selected = book.accounts.first().map(|a| a.uuid.clone()).unwrap_or_default();
    }
    save(&book)?;
    Ok(views(&book))
}

#[derive(Serialize)]
pub struct DeviceCode {
    user_code: String,
    verification_uri: String,
    device_code: String,
    interval: u64,
    expires_in: u64,
}

#[tauri::command]
pub async fn login_start() -> Result<DeviceCode, String> {
    let cid = client_id()?;
    LOGIN.fetch_add(1, Ordering::SeqCst);
    let v: Value = client()
        .post(DEVICE_CODE)
        .form(&[("client_id", cid.as_str()), ("scope", SCOPE)])
        .send()
        .await
        .map_err(|err| format!("Couldn't reach Microsoft ({err})"))?
        .json()
        .await
        .map_err(e)?;
    if let Some(msg) = v.get("error_description").and_then(Value::as_str) {
        return Err(first_line(msg));
    }
    Ok(DeviceCode {
        user_code: text(&v, "user_code"),
        verification_uri: text(&v, "verification_uri"),
        device_code: text(&v, "device_code"),
        interval: v.get("interval").and_then(Value::as_u64).unwrap_or(5),
        expires_in: v.get("expires_in").and_then(Value::as_u64).unwrap_or(900),
    })
}

#[tauri::command]
pub fn login_cancel() {
    LOGIN.fetch_add(1, Ordering::SeqCst);
}

#[tauri::command]
pub async fn login_poll(device_code: String, interval: u64, expires_in: u64) -> Result<Vec<AccountView>, String> {
    let cid = client_id()?;
    let ticket = LOGIN.load(Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(expires_in.clamp(60, 1800));
    let mut wait = interval.clamp(1, 30);
    let (access, refresh) = loop {
        tokio::time::sleep(Duration::from_secs(wait)).await;
        if LOGIN.load(Ordering::SeqCst) != ticket {
            return Err("cancelled".into());
        }
        if Instant::now() > deadline {
            return Err("The code expired. Close this and try again.".into());
        }
        let form = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", cid.as_str()),
            ("device_code", device_code.as_str()),
        ];
        // A dropped connection while waiting is not a failure; just ask again.
        let Ok(resp) = client().post(TOKEN).form(&form).send().await else { continue };
        let Ok(v) = resp.json::<Value>().await else { continue };
        if let Some(token) = v.get("access_token").and_then(Value::as_str) {
            break (token.to_string(), text(&v, "refresh_token"));
        }
        match v.get("error").and_then(Value::as_str) {
            Some("authorization_pending") => {}
            Some("slow_down") => wait += 5,
            Some("authorization_declined") => return Err("Sign-in was declined.".into()),
            Some("expired_token") => return Err("The code expired. Close this and try again.".into()),
            _ => return Err(v.get("error_description").and_then(Value::as_str).map(first_line).unwrap_or_else(|| "Microsoft sign-in failed.".into())),
        }
    };
    let account = minecraft(&access, refresh).await?;
    let mut book = load();
    book.accounts.retain(|a| a.uuid != account.uuid);
    book.selected = account.uuid.clone();
    book.accounts.push(account);
    save(&book)?;
    Ok(views(&book))
}

async fn xbox(url: &str, body: Value) -> Result<Value, String> {
    let resp = client()
        .post(url)
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("Couldn't reach Xbox Live ({err})"))?;
    let status = resp.status();
    let v: Value = resp.json().await.unwrap_or(Value::Null);
    if status.is_success() {
        return Ok(v);
    }
    Err(match v.get("XErr").and_then(Value::as_u64) {
        Some(2148916233) => "This Microsoft account has no Xbox profile yet. Sign in once at xbox.com, then try again.".into(),
        Some(2148916235) => "Xbox Live isn't available in this account's country.".into(),
        Some(2148916236) | Some(2148916237) => "This account needs adult verification on xbox.com first.".into(),
        Some(2148916238) => "This is a child account: an adult has to add it to a Microsoft family first.".into(),
        _ => format!("Xbox sign-in failed ({status})."),
    })
}

/// Reads one claim from a JWT's payload (base64url JSON) without checking it; used only for the Xbox user id.
fn jwt_claim(token: &str, key: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let mut bytes = Vec::with_capacity(payload.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in payload.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => break,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    v.get(key).map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string()))
}

/// Microsoft token -> Xbox Live -> XSTS -> Minecraft token -> the player's profile.
async fn minecraft(ms_access: &str, refresh: String) -> Result<Account, String> {
    let xbl = xbox(
        "https://user.auth.xboxlive.com/user/authenticate",
        json!({
            "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={ms_access}") },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }),
    )
    .await?;
    let xsts = xbox(
        "https://xsts.auth.xboxlive.com/xsts/authorize",
        json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [text(&xbl, "Token")] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }),
    )
    .await?;
    let uhs = xsts.pointer("/DisplayClaims/xui/0/uhs").and_then(Value::as_str).unwrap_or_default();
    let resp = client()
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&json!({ "identityToken": format!("XBL3.0 x={uhs};{}", text(&xsts, "Token")) }))
        .send()
        .await
        .map_err(|err| format!("Couldn't reach Minecraft's servers ({err})"))?;
    if resp.status().as_u16() == 403 {
        return Err(NOT_APPROVED.into());
    }
    if !resp.status().is_success() {
        return Err(format!("Minecraft sign-in failed ({}).", resp.status()));
    }
    let mc: Value = resp.json().await.map_err(e)?;
    let token = text(&mc, "access_token");
    let expires = crate::mc::now() + mc.get("expires_in").and_then(Value::as_u64).unwrap_or(86_400);

    let resp = client()
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|err| format!("Couldn't reach Minecraft's servers ({err})"))?;
    if resp.status().as_u16() == 404 {
        return Err("This Microsoft account doesn't own Minecraft: Java Edition.".into());
    }
    if !resp.status().is_success() {
        return Err(format!("Couldn't read the Minecraft profile ({}).", resp.status()));
    }
    let profile: Value = resp.json().await.map_err(e)?;
    Ok(Account {
        uuid: text(&profile, "id"),
        name: text(&profile, "name"),
        xuid: jwt_claim(&token, "xuid").unwrap_or_default(),
        refresh_token: refresh,
        mc_token: token,
        mc_expires: expires,
    })
}

/// The selected account with a Minecraft token good for the session, refreshed quietly when it is near expiry.
pub async fn account_for_launch() -> Result<Account, String> {
    let mut book = load();
    let idx = book
        .accounts
        .iter()
        .position(|a| a.uuid == book.selected)
        .or(if book.accounts.is_empty() { None } else { Some(0) })
        .ok_or("Add your Microsoft account (the account button at the top right) before playing.")?;
    if book.accounts[idx].mc_expires > crate::mc::now() + 600 {
        return Ok(book.accounts[idx].clone());
    }
    let cid = client_id()?;
    let refresh = book.accounts[idx].refresh_token.clone();
    let form = [("grant_type", "refresh_token"), ("client_id", cid.as_str()), ("refresh_token", refresh.as_str()), ("scope", SCOPE)];
    let v: Value = client()
        .post(TOKEN)
        .form(&form)
        .send()
        .await
        .map_err(|err| format!("Couldn't reach Microsoft ({err})"))?
        .json()
        .await
        .map_err(e)?;
    let Some(access) = v.get("access_token").and_then(Value::as_str) else {
        return Err("Your Microsoft sign-in has expired. Sign out of the account (top right) and add it again.".into());
    };
    let fresh = minecraft(access, opt_refresh(&v).unwrap_or(refresh)).await?;
    book.accounts[idx] = fresh.clone();
    book.selected = fresh.uuid.clone();
    save(&book)?;
    Ok(fresh)
}

fn opt_refresh(v: &Value) -> Option<String> {
    v.get("refresh_token").and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from)
}
