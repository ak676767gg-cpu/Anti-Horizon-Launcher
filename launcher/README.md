# Anti Horizon Launcher

A fast Minecraft: Java Edition launcher in the Anti-Horizon gold-and-space look, for Windows 10 and 11.
Download it at **[anti-horizon.com/launcher](https://anti-horizon.com/launcher/)**.

- Every version from Mojang, with Vanilla, Fabric, Quilt, Forge or NeoForge
- Mods, resource packs, shaders, data packs and modpacks from Modrinth (and CurseForge), plus everything they need
- An instance screen for each setup: its content, files and worlds, and sharing it as a `.mrpack`
- A Skins tab: Minecraft's default skins, your own skin packs, or upload one
- Mojang's own Java for each version, picked automatically
- Updates itself, with every update signed and checked
- Everything lives in `%APPDATA%\AntiHorizonLauncher`. It never touches your normal `.minecraft` folder.

> Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.

## Why you can trust it

A launcher signs in to your Microsoft account, so you should be able to see exactly what it does with it. All of it is
in this folder, and none of it is obfuscated.

| | |
| --- | --- |
| **Signing in** | You sign in on **Microsoft's own page** with a short code ([auth.rs](src-tauri/src/auth.rs)). The launcher never sees or stores your password. |
| **Your sign-in afterwards** | The tokens Microsoft and Minecraft give back are kept only on your PC, encrypted with Windows' own protection (DPAPI), so only your Windows user can read them. They are sent only to Microsoft, Xbox and Minecraft's sign-in services, and to Minecraft itself when it starts. |
| **Who it talks to** | Microsoft, Xbox and Minecraft services (sign-in, your skin); Mojang (the game, its libraries and assets); Fabric, Quilt, Forge and NeoForge (mod loaders); Modrinth and CurseForge (mods); and anti-horizon.com (launcher updates). Plus the download links those services hand out. That's the full list: there is **no telemetry, no analytics and no tracking**. |
| **Downloads** | Game files are checked against the fingerprints (SHA-1) Mojang and Modrinth publish before they're used ([net.rs](src-tauri/src/net.rs)). |
| **Updates** | Every update is signed. The launcher refuses an update whose signature doesn't match the key it was built with ([update.rs](src-tauri/src/update.rs)). |
| **Your computer** | It works inside its own folder. Deleting an instance or a world sends it to the Recycle Bin, never straight to gone. |

**Copies built from this code can't sign in.** The Microsoft sign-in ID that Mojang approved is built into official
releases only (`release.ps1` reads it from a private file), so nobody can make a lookalike that signs in as the Anti
Horizon Launcher. Only download it from anti-horizon.com.

**Found a security problem?** Please report it privately: see [SECURITY.md](SECURITY.md).

## How it's made

Rust ([Tauri 2](https://tauri.app)) does the work: version lists, downloads, Java, Microsoft sign-in and launching.
The window is HTML, CSS and JavaScript in `dist/`, shown by WebView2, the Edge engine built into Windows. The Modrinth
App is built the same way.

| File | Job |
| --- | --- |
| `src-tauri/src/mc.rs` | Versions, instances, installing, launching |
| `src-tauri/src/mods.rs`, `installed.rs` | Modrinth and CurseForge, installed mods and their updates |
| `src-tauri/src/deps.rs` | Adds anything your mods need, so the game doesn't stop at "missing mods" |
| `src-tauri/src/content.rs` | The instance screen: data packs, files, worlds, `.mrpack` export and import |
| `src-tauri/src/skins.rs` | The Skins tab, through Minecraft's own profile service |
| `src-tauri/src/auth.rs` | Microsoft sign-in, tokens encrypted with DPAPI |
| `src-tauri/src/net.rs` | Parallel, verified downloads |
| `src-tauri/src/update.rs` | Signed self-updates |
| `dist/` | The window |

## Build it yourself

You need Rust and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for Windows. The Anti-Horizon
client ships inside the launcher: build it first (see [../client](../client)) and copy its jar to
`src-tauri/presets/anti-horizon-client.jar`.

```
cargo build --release --manifest-path src-tauri/Cargo.toml
```

To sign in, your build needs a Microsoft app of its own, approved by Mojang (`AH_MSA_CLIENT_ID` when you build). For
CurseForge, a CurseForge API key (`AH_CURSEFORGE_KEY`). Modrinth needs nothing.

## License

**Source available, not open source.** Under the [PolyForm Strict License 1.0.0](LICENSE): you may read this code,
learn from it and build it for noncommercial use, but you may not copy it into your own projects, change it, or share
or re-upload it. The fonts are under their own license; see [NOTICE.md](NOTICE.md).

Copyright © 2026 AK, [Anti-Horizon](https://anti-horizon.com).
