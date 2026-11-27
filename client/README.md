# Anti-Horizon Client

A Fabric client mod for Minecraft: Java Edition, made for the [Anti-Horizon Launcher](https://anti-horizon.com/launcher/).
One jar works on **1.21.11, 26.1, 26.2 and 26.3**.

- **Right Shift menu**: your HUD pieces and installed mods on one page. Mods that support it switch on and off
  straight away (Iris shaders, 3D Skin Layers, Dynamic FPS, Entity Culling), and any mod with a settings screen opens
  from there.
- **HUD**: FPS, coordinates, direction, ping, server, CPS, combo counter, reach, saturation (no AppleSkin needed),
  armour, item tracker and keystrokes. Move and resize each one in the HUD editor.
- **Aurora background**: Minecraft's own title screen, with an aurora video behind it instead of the panorama.

Needs Fabric Loader 0.20.1 or newer and Fabric API.

> Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.

## Why you can trust it

A client mod runs inside your game, so you should know exactly what it does. Every line is in this repository, and
none of it is obfuscated.

| | |
| --- | --- |
| **Your account** | The mod never touches your Microsoft or Minecraft login, session or access token. The only thing it reads from your account is your **username**, to show it in the menu ([ClientMenu.java](src/client/java/com/antihorizon/client/ClientMenu.java)). |
| **The internet** | Two connections, both to Anti-Horizon. The first time the title screen opens, it downloads the aurora video from `https://anti-horizon.com/client/menu-background-1.ahv`, checks it against a fixed SHA-256 fingerprint, and keeps it in `~/.anti-horizon` ([MenuVideo.java](src/client/java/com/antihorizon/client/MenuVideo.java)). And for cosmetics, it asks `https://cosmetics.anti-horizon.com` which cape, wings and head piece the players around you wear, sending only their player IDs ([Cosmetics.java](src/client/java/com/antihorizon/client/Cosmetics.java)); switch Cosmetics off in the menu and it stops. Nothing else is ever sent: no telemetry, no analytics, no tracking. |
| **Cosmetics** | Capes go through Minecraft's own cape drawing ([SkinMixin.java](src/client/java/com/antihorizon/client/mixin/SkinMixin.java)); wings and head pieces are a render layer ([CosmeticsLayer.java](src/client/java/com/antihorizon/client/CosmeticsLayer.java)). They only change what you see; nothing about them reaches the server you're on. You pick yours in the launcher. |
| **What it changes in Minecraft** | One mixin, which swaps the title-screen panorama for the video ([PanoramaMixin.java](src/client/java/com/antihorizon/client/mixin/PanoramaMixin.java)). Everything else goes through Fabric API's normal hooks: the HUD, the menu key, and the attack event the combo counter and reach display use. |
| **What it reads** | Only your own game on your PC: frame rate, position, armour, hunger, and the keys and mouse buttons you press (for keystrokes and CPS), all to draw the HUD. Nothing is recorded or sent anywhere. |
| **What it saves** | Your settings, in `config/antihorizon-client.properties`. |
| **Other mods** | The live switches use Java reflection to call those mods' own public settings, and a mod's settings screen opens through Mod Menu's standard hook. They only do something when you click them. See [LiveMods.java](src/client/java/com/antihorizon/client/LiveMods.java). |

**Check a download.** Every release lists the SHA-256 fingerprint of its jar. On Windows, compare it with:

```powershell
Get-FileHash .\anti-horizon-client.jar -Algorithm SHA256
```

If the fingerprints don't match, don't use that file.

**Found a security problem?** Please report it privately: see [SECURITY.md](SECURITY.md).

## Install

The Anti-Horizon Launcher adds the client to every Fabric instance on a supported version for you. To use it anywhere
else, put the jar from [Releases](https://github.com/ak676767gg-cpu/Anti-Horizon-Launcher/releases) and [Fabric API](https://modrinth.com/mod/fabric-api) in your
instance's `mods` folder. Get it only from this repository or anti-horizon.com.

## Build it yourself

You need Java 25 (set `JAVA_HOME`) and Python 3.

```powershell
.\gradlew.bat build           # the 1.21.11 build, in build/libs
python tools\build-all.py     # the one jar for 1.21.11 to 26.3, build/libs/anti-horizon-client.jar
```

The 1.21.11 build uses Fabric Loom. The 26.x builds are made from the same code by
[tools/port26.py](tools/port26.py), which applies Minecraft 26's renames and compiles against the 26.2 and 26.3 game
files from the Modrinth app (Minecraft 26 isn't obfuscated, so they need no remapping). A small start-up class,
[Boot.java](src/client/java/com/antihorizon/boot/Boot.java), loads the build that matches the running game.

## License

**Source available, not open source.** This code is public so you can read it, check what it does and learn from it.
It is under the [PolyForm Strict License 1.0.0](LICENSE):

- ✅ Read it, learn from it, build it, and use the mod for anything noncommercial.
- ❌ Copy the code into your own projects, change it, or share or re-upload it (including the jar).

Ideas aren't owned by anyone, so building something similar yourself, from scratch, is fine. The Chakra Petch fonts
are under their own license; see [NOTICE.md](NOTICE.md).

Copyright © 2026 AK, [Anti-Horizon](https://anti-horizon.com).
