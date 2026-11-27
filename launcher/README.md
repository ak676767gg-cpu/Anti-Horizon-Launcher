# Anti Horizon Launcher

A fast Minecraft: Java Edition launcher in the Anti-Horizon gold-and-space look, for Windows 10 and 11, macOS 10.15
or newer (Apple silicon and Intel) and 64-bit Linux (AppImage, .deb and .rpm).
Download it at **[anti-horizon.com](https://anti-horizon.com/#download)**.

- Every version from Mojang, with Vanilla, Fabric, Quilt, Forge or NeoForge
- Mods, resource packs, shaders, data packs and modpacks from Modrinth (and CurseForge), plus everything they need
- An instance screen for each setup: its content, files and worlds, and sharing it as a `.mrpack`
- A Skins tab: Minecraft's default skins, your own skin packs, or upload one
- Mojang's own Java for each version, picked automatically
- Updates itself, with every update signed and checked
- Everything lives in its own `AntiHorizonLauncher` folder: in `%APPDATA%` on Windows, `~/Library/Application Support`
  on a Mac and `~/.local/share` on Linux. It never touches your normal `.minecraft` folder.

> Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.

## Why you can trust it

A launcher signs in to your Microsoft account, so you should be able to see exactly what it does with it. All of it is
in this folder, and none of it is obfuscated.

| | |
| --- | --- |
| **Signing in** | You sign in on **Microsoft's own page** with a short code ([auth.rs](src-tauri/src/auth.rs)). The launcher never sees or stores your password. |
| **Your sign-in afterwards** | The tokens Microsoft and Minecraft give back are kept only on your computer. On Windows they're encrypted with Windows' own protection (DPAPI), so only your Windows user can read them; on a Mac or Linux the file is created so only your user can open it. They are sent only to Microsoft, Xbox and Minecraft's sign-in services, and to Minecraft itself when it starts. |
| **Who it talks to** | Microsoft, Xbox and Minecraft services (sign-in, your skin); Mojang (the game, its libraries and assets); Fabric, Quilt, Forge and NeoForge (mod loaders); Modrinth and CurseForge (mods); anti-horizon.com and, for Mac and Linux, this repository's releases (launcher updates); and Anti-Horizon's cosmetics service (the cosmetics you wear). Plus the download links those services hand out. That's the full list: there is **no telemetry, no analytics and no tracking**. |
| **Downloads** | Game files are checked against the fingerprints (SHA-1) Mojang and Modrinth publish before they're used ([net.rs](src-tauri/src/net.rs)). |
| **Updates** | Every update is signed. The launcher refuses an update whose signature doesn't match the key it was built with ([update.rs](src-tauri/src/update.rs)). |
| **Your computer** | It works inside its own folder. Deleting an instance or a world sends it to the Recycle Bin or the Trash, never straight to gone. |
| **Cosmetics** | Picking cosmetics signs you in to Anti-Horizon's cosmetics service the way a Minecraft server checks you: Mojang confirms the account is yours, and your Minecraft token only ever goes to Mojang ([cosmetics.rs](src-tauri/src/cosmetics.rs), the service in [../cosmetics](../cosmetics)). The service keeps your player ID and the cosmetics you picked, nothing else. |
| **Who built it** | The Mac and Linux downloads are built by GitHub from this repository ([build.yml](../.github/workflows/build.yml)), with a SHA-256 for each file and a signed build record: `gh attestation verify <file> --repo ak676767gg-cpu/Anti-Horizon-Launcher`. |

**Copies built from this code can't sign in.** The Microsoft sign-in ID that Mojang approved is built into official
releases only (from a private file on the developer's computer, or a private secret for GitHub's builds), so nobody
can make a lookalike that signs in as the Anti Horizon Launcher. Only download it from anti-horizon.com or this
repository's releases.

**Found a security problem?** Please report it privately: see [SECURITY.md](SECURITY.md).

## How it's made

Rust ([Tauri 2](https://tauri.app)) does the work: version lists, downloads, Java, Microsoft sign-in and launching.
The window is HTML, CSS and JavaScript in `dist/`, shown by the web engine built into the system: WebView2 on Windows,
WebKit on a Mac and WebKitGTK on Linux. The Modrinth App is built the same way.

| File | Job |
| --- | --- |
| `src-tauri/src/mc.rs` | Versions, instances, installing, launching |
| `src-tauri/src/mods.rs`, `installed.rs` | Modrinth and CurseForge, installed mods and their updates |
| `src-tauri/src/deps.rs` | Adds anything your mods need, so the game doesn't stop at "missing mods" |
| `src-tauri/src/content.rs` | The instance screen: data packs, files, worlds, `.mrpack` export and import |
| `src-tauri/src/skins.rs` | The Skins tab, through Minecraft's own profile service |
| `src-tauri/src/auth.rs` | Microsoft sign-in, tokens encrypted with DPAPI on Windows |
| `src-tauri/src/platform.rs` | What differs between Windows, macOS and Linux |
| `src-tauri/src/net.rs` | Parallel, verified downloads |
| `src-tauri/src/update.rs` | Signed self-updates |
| `dist/` | The window |

## Build it yourself

You need Rust and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your system. The Anti-Horizon
client ships inside the launcher; its built jar is in `src-tauri/presets/anti-horizon-client.jar`, made from
[../client](../client).

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
