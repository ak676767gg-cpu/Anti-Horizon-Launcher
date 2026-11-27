// Small fixes the launcher makes to an instance's mod settings just before it starts. Each has a switch in Settings.
use std::path::Path;

/// JEI says in chat, on every server that doesn't run JEI itself, that the server didn't send it recipes. It's
/// harmless (JEI shows the game's own recipes instead) but it appears every time, so the launcher turns that one
/// message off in JEI's own settings. Nothing else in JEI changes.
pub fn quiet_jei(game: &Path) {
    let has_jei = std::fs::read_dir(game.join("mods")).into_iter().flatten().flatten().any(|e| {
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        name.starts_with("jei-") && name.ends_with(".jar")
    });
    if !has_jei {
        return;
    }
    let file = game.join("config").join("jei").join("client").join("default").join("jei-client.ini");
    match std::fs::read_to_string(&file) {
        Ok(text) => {
            if text.contains("recipeSyncWarningEnabled = true") {
                let _ = std::fs::write(&file, text.replace("recipeSyncWarningEnabled = true", "recipeSyncWarningEnabled = false"));
            }
        }
        // JEI hasn't written its settings yet: start the file with this one, and JEI adds the rest on first start.
        Err(_) => {
            if let Some(dir) = file.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&file, "[advanced]\n\trecipeSyncWarningEnabled = false\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::quiet_jei;

    #[test]
    fn jei_warning_is_switched_off() {
        let dir = std::env::temp_dir().join(format!("ah-tweaks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = dir.join("config/jei/client/default/jei-client.ini");

        // No JEI: nothing is written.
        std::fs::create_dir_all(dir.join("mods")).unwrap();
        quiet_jei(&dir);
        assert!(!cfg.exists());

        // JEI, no settings yet: a file with just this setting.
        std::fs::write(dir.join("mods/jei-1.21.11-fabric-27.48.0.106.jar"), b"").unwrap();
        quiet_jei(&dir);
        assert!(std::fs::read_to_string(&cfg).unwrap().contains("recipeSyncWarningEnabled = false"));

        // JEI's own full file: only that line changes.
        std::fs::write(&cfg, "[search]\n\tx = 1\n[advanced]\n\trecipeSyncWarningEnabled = true\n\ty = 2\n").unwrap();
        quiet_jei(&dir);
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "[search]\n\tx = 1\n[advanced]\n\trecipeSyncWarningEnabled = false\n\ty = 2\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
