# Anti-Horizon Launcher and Client

The source of the Anti Horizon Launcher and the Anti-Horizon client for Minecraft: Java Edition, public so anyone can
read it and check exactly what it does.

| | |
| --- | --- |
| **[launcher/](launcher)** | The Anti Horizon Launcher for Windows, Mac and Linux: instances, mods, skins, Microsoft sign-in. Rust (Tauri 2) and a web window. Download it at [anti-horizon.com](https://anti-horizon.com/#download) or from [Releases](https://github.com/ak676767gg-cpu/Anti-Horizon-Launcher/releases). |
| **[client/](client)** | The Anti-Horizon client: a Fabric mod with the Right Shift menu, 12 HUD modules and the aurora title background. One jar for Minecraft 1.21.11, 26.1, 26.2 and 26.3. |

> Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.

## Why you can trust them

Both READMEs say exactly what each one does with your account and your computer:
[the launcher](launcher/README.md#why-you-can-trust-it) and [the client](client/README.md#why-you-can-trust-it).
In short:

- **Your password is never seen.** You sign in on Microsoft's own page; the sign-in it keeps stays on your computer, readable only by you.
- **Built in the open.** GitHub builds the Mac and Linux versions from this code ([build.yml](.github/workflows/build.yml)), and every download comes with its SHA-256 and a signed build record.
- **No telemetry, no analytics, no tracking.** Each README lists every server it talks to.
- **Nothing is hidden.** No obfuscated code; updates are signed; downloads are checked against their fingerprints.
- **Only official copies can sign in**, so a lookalike built from this code can't pretend to be the launcher.

Get them only from [anti-horizon.com](https://anti-horizon.com) or this repository's releases. Found a security problem? Please
report it privately: see [SECURITY.md](SECURITY.md).

## License

**Source available, not open source.** Everything here is under the [PolyForm Strict License 1.0.0](LICENSE): you may
read it, learn from it and build it for noncommercial use, but you may not copy it into your own projects, change it,
or share or re-upload it. Fonts are under their own licenses (see each folder's NOTICE.md).

Copyright © 2026 AK, [Anti-Horizon](https://anti-horizon.com).
