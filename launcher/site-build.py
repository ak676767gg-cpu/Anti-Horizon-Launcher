"""
Builds anti-horizon.com into site-out/: the pages in site/, plus the files installed launchers and clients depend on,
which are kept out of the code because of their size:

  launcher/latest.json and launcher/Anti-Horizon-Launcher-Setup.exe   from release/updates (release.ps1 makes them,
                                                                      release-other.py adds Mac and Linux)
  client/menu-background-1.ahv                                        from site-assets/
  launcher/clips/*.mp4 and client/clips/*.ahv                         from site-assets/clips (clips/make-clips.py)

The Mac and Linux downloads stay on GitHub's release page (Linux's are too big for Cloudflare Pages); the site links
to them, so latest.json has to list them before the site is built.

Publish: npx wrangler pages deploy site-out --project-name anti-horizon --branch main --commit-dirty=true
"""
import json
import os
import shutil

ROOT = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(ROOT, "site-out")
UPDATES = os.path.join(ROOT, "release", "updates")
ASSETS = os.path.join(ROOT, "site-assets")

shutil.rmtree(OUT, ignore_errors=True)
shutil.copytree(os.path.join(ROOT, "site"), OUT)

os.makedirs(os.path.join(OUT, "launcher"))
for f in ("latest.json", "Anti-Horizon-Launcher-Setup.exe"):
    shutil.copy2(os.path.join(UPDATES, f), os.path.join(OUT, "launcher", f))
os.makedirs(os.path.join(OUT, "client"))
shutil.copy2(os.path.join(ASSETS, "menu-background-1.ahv"), os.path.join(OUT, "client", "menu-background-1.ahv"))
# The live clips (clips/make-clips.py): launcher backgrounds and the client's live capes.
os.makedirs(os.path.join(OUT, "launcher", "clips"))
os.makedirs(os.path.join(OUT, "client", "clips"))
for f in sorted(os.listdir(os.path.join(ASSETS, "clips"))):
    side = "client" if f.endswith(".ahv") else "launcher"
    shutil.copy2(os.path.join(ASSETS, "clips", f), os.path.join(OUT, side, "clips", f))

latest = json.load(open(os.path.join(UPDATES, "latest.json"), encoding="utf-8"))
version = latest["version"]
missing = [p for p in ("windows-x86_64", "darwin-aarch64", "darwin-x86_64", "linux-x86_64") if p not in latest["platforms"]]
if missing:
    raise SystemExit(f"latest.json has no {', '.join(missing)} for {version} yet: run release-other.py first.")
sizes = json.load(open(os.path.join(UPDATES, "other", "sizes.json"), encoding="utf-8"))
size = os.path.getsize(os.path.join(UPDATES, "Anti-Horizon-Launcher-Setup.exe")) / 1048576
index = os.path.join(OUT, "index.html")
html = open(index, encoding="utf-8").read()
for key, value in {
    "VERSION": version,
    "SIZE": f"{size:.1f}",
    "MAC_SIZE": f"{sizes['Anti-Horizon-Launcher-mac.dmg'] / 1048576:.1f}",
    "LINUX_SIZE": f"{sizes['Anti-Horizon-Launcher-linux-x86_64.AppImage'] / 1048576:.0f}",
}.items():
    html = html.replace("{{" + key + "}}", value)
if "{{" in html:
    raise SystemExit("index.html still has a {{placeholder}} nobody filled in.")
open(index, "w", encoding="utf-8", newline="\n").write(html)

big = [(os.path.relpath(os.path.join(d, f), OUT), os.path.getsize(os.path.join(d, f))) for d, _, fs in os.walk(OUT) for f in fs]
over = [p for p, s in big if s > 25 * 1048576]
if over:
    raise SystemExit(f"Cloudflare Pages refuses files over 25 MiB: {over}")
print(f"site-out: {len(big)} files, launcher {version} ({size:.1f} MB), {sum(s for _, s in big) / 1048576:.1f} MB in all")
