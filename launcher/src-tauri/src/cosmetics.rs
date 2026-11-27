// Anti-Horizon cosmetics: the cape, wings and head piece you wear, saved with the cosmetics service so other players
// with the Anti-Horizon client see them too.
//
// Signing in to the service works the way joining a Minecraft server does: the service hands out a one-time server
// ID, the launcher tells Mojang this account is joining with it, and the service asks Mojang to confirm. Your
// Minecraft token only ever goes to Mojang. The service's own token (it can change your cosmetics, nothing else) is
// kept for 30 days in the launcher's folder.
//
// After a change the launcher also writes ~/.anti-horizon/cosmetics-me.json, which the client mod watches, so the
// new look shows in a game that's already open within a couple of seconds.
use crate::net::client;
use crate::store::{e, read_json, root, write_json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

const SERVICE: &str = "https://cosmetics.anti-horizon.com";
const JOIN: &str = "https://sessionserver.mojang.com/session/minecraft/join";

fn service() -> String {
    // The development copy can be pointed at a local copy of the service; releases always use the real one.
    #[cfg(debug_assertions)]
    if let Ok(url) = std::env::var("AH_COSMETICS") {
        return url.trim_end_matches('/').to_string();
    }
    SERVICE.to_string()
}

fn join_url() -> String {
    #[cfg(debug_assertions)]
    if let Ok(url) = std::env::var("AH_MOJANG_JOIN") {
        return url;
    }
    JOIN.to_string()
}

/// One player's cosmetics: the ID of each, or nothing.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Debug)]
pub struct Look {
    #[serde(default)]
    pub cape: Option<String>,
    #[serde(default)]
    pub wings: Option<String>,
    #[serde(default)]
    pub head: Option<String>,
}

#[derive(Serialize)]
pub struct Mine {
    /// The account's name, or nothing when no account is added yet (the window then only previews).
    name: Option<String>,
    look: Look,
}

#[derive(Serialize, Deserialize, Clone)]
struct Token {
    token: String,
    expires: u64,
}

fn tokens_path() -> std::path::PathBuf {
    root().join("cosmetics-tokens.json")
}

/// A service token for this account: the saved one while it has a day left, else a new sign-in.
async fn token(account: &crate::auth::Account, fresh: bool) -> Result<String, String> {
    let mut saved: HashMap<String, Token> = read_json(&tokens_path()).unwrap_or_default();
    if !fresh {
        if let Some(t) = saved.get(&account.uuid) {
            if t.expires > crate::mc::now() + 86_400 {
                return Ok(t.token.clone());
            }
        }
    }
    let svc = service();
    let unreachable = |err: reqwest::Error| format!("Couldn't reach the cosmetics service ({err})");
    let start: Value = client()
        .post(format!("{svc}/v1/login/start"))
        .json(&json!({ "name": account.name }))
        .send()
        .await
        .map_err(unreachable)?
        .error_for_status()
        .map_err(unreachable)?
        .json()
        .await
        .map_err(e)?;
    let server_id = start.get("serverId").and_then(Value::as_str).ok_or("The cosmetics service didn't answer properly.")?;
    let joined = client()
        .post(join_url())
        .json(&json!({ "accessToken": account.mc_token, "selectedProfile": account.uuid.replace('-', ""), "serverId": server_id }))
        .send()
        .await
        .map_err(|err| format!("Couldn't reach Mojang ({err})"))?;
    if !joined.status().is_success() {
        return Err(format!("Mojang didn't accept the sign-in (HTTP {}). Try again in a minute.", joined.status().as_u16()));
    }
    let done = client()
        .post(format!("{svc}/v1/login/finish"))
        .json(&json!({ "name": account.name, "serverId": server_id }))
        .send()
        .await
        .map_err(unreachable)?;
    if !done.status().is_success() {
        return Err(format!("The cosmetics service couldn't confirm your account (HTTP {}).", done.status().as_u16()));
    }
    let v: Value = done.json().await.map_err(e)?;
    let t = Token {
        token: v.get("token").and_then(Value::as_str).ok_or("No token came back.")?.to_string(),
        expires: v.get("expires").and_then(Value::as_u64).unwrap_or(0),
    };
    saved.insert(account.uuid.clone(), t.clone());
    write_json(&tokens_path(), &saved)?;
    Ok(t.token)
}

/// Calls the service as this account, signing in again once if the saved token was turned away.
async fn signed(account: &crate::auth::Account, method: reqwest::Method, body: Option<&Look>) -> Result<Look, String> {
    for attempt in 0..2 {
        let t = token(account, attempt == 1).await?;
        let mut req = client().request(method.clone(), format!("{}/v1/me", service())).bearer_auth(&t);
        if let Some(b) = body {
            req = req.json(b);
        }
        let res = req.send().await.map_err(|err| format!("Couldn't reach the cosmetics service ({err})"))?;
        if res.status().as_u16() == 401 && attempt == 0 {
            continue;
        }
        if !res.status().is_success() {
            let msg = res.json::<Value>().await.ok().and_then(|v| v.get("error").and_then(Value::as_str).map(String::from));
            return Err(msg.unwrap_or_else(|| "The cosmetics service said no.".into()));
        }
        return res.json::<Look>().await.map_err(e);
    }
    Err("The cosmetics service didn't accept the sign-in.".into())
}

/// Tells an open game straight away (the client mod watches this file).
fn tell_game(uuid: &str, look: &Look) {
    let path = crate::platform::home().join(".anti-horizon").join("cosmetics-me.json");
    let mut v = serde_json::to_value(look).unwrap_or_default();
    v["uuid"] = json!(uuid);
    let _ = write_json(&path, &v);
}

/// The selected account's cosmetics. With no account yet, an empty look and no name: the window can still preview.
#[tauri::command]
pub async fn cosmetics_get() -> Result<Mine, String> {
    let Ok(account) = crate::auth::account_for_launch().await else {
        return Ok(Mine { name: None, look: Look::default() });
    };
    let look = signed(&account, reqwest::Method::GET, None).await?;
    Ok(Mine { name: Some(account.name), look })
}

/// Wears these cosmetics on the selected account.
#[tauri::command]
pub async fn cosmetics_set(look: Look) -> Result<Look, String> {
    let account = crate::auth::account_for_launch().await?;
    let saved = signed(&account, reqwest::Method::PUT, Some(&look)).await?;
    tell_game(&account.uuid, &saved);
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Against a local copy of the service and a stand-in for Mojang:
    ///   AH_COSMETICS=http://127.0.0.1:8787 AH_MOJANG_JOIN=http://127.0.0.1:8799/join cargo test cosmetics -- --ignored
    #[test]
    #[ignore]
    fn sign_in_and_wear() {
        let dir = std::env::temp_dir().join(format!("ah-cosmetics-test-{}", crate::mc::now()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("APPDATA", &dir);
        let account = crate::auth::Account {
            uuid: "0123456789abcdef0123456789abcdef".into(),
            name: "TestPlayer".into(),
            mc_token: "test-token".into(),
            ..Default::default()
        };
        tauri::async_runtime::block_on(async {
            let want = Look { cape: Some("torus".into()), wings: Some("gold".into()), head: Some("crown".into()) };
            let got = signed(&account, reqwest::Method::PUT, Some(&want)).await.expect("put");
            assert_eq!(got, want);
            let back = signed(&account, reqwest::Method::GET, None).await.expect("get");
            assert_eq!(back, want);
            // A saved token is used again without signing in.
            let saved: HashMap<String, Token> = read_json(&tokens_path()).unwrap();
            assert!(saved.contains_key(&account.uuid));
            let none = Look::default();
            assert_eq!(signed(&account, reqwest::Method::PUT, Some(&none)).await.expect("clear"), none);
        });
    }
}
