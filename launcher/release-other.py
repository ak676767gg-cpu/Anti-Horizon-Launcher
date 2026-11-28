"""
Adds the Mac and Linux builds of a launcher release to release/updates/latest.json, after release.ps1 (Windows) and
`publish-public.py --release` (which makes GitHub build Mac and Linux from the public code).

It downloads the update files from the GitHub release, checks them against the release's SHA256SUMS.txt, signs each
one with the launcher's update key (which stays on this computer, in %USERPROFILE%\\.anti-horizon), and lists them in
latest.json, so installed Mac and Linux launchers update the same way Windows ones do. Then publish the site.

    python release-other.py            waits for GitHub's build if it is still running (up to 45 minutes)
"""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request

ROOT = os.path.dirname(os.path.abspath(__file__))
REPO = "ak676767gg-cpu/Anti-Horizon-Launcher"
KEYS = os.path.join(os.path.expanduser("~"), ".anti-horizon")
UPDATES = os.path.join(ROOT, "release", "updates")
OUT = os.path.join(UPDATES, "other")
AGENT = {"User-Agent": "AntiHorizonRelease/1", "Accept": "application/vnd.github+json"}

MAC = "Anti-Horizon-Launcher-mac.app.tar.gz"
APPIMAGE = "Anti-Horizon-Launcher-linux-x86_64.AppImage"
DEB = "Anti-Horizon-Launcher-linux-amd64.deb"
RPM = "Anti-Horizon-Launcher-linux-x86_64.rpm"
# The updater looks for "<os>-<arch>-<kind of install>" first, then "<os>-<arch>".
PLATFORMS = {
    "darwin-aarch64": MAC,
    "darwin-x86_64": MAC,
    "linux-x86_64": APPIMAGE,
    "linux-x86_64-appimage": APPIMAGE,
    "linux-x86_64-deb": DEB,
    "linux-x86_64-rpm": RPM,
}


def get(url):
    with urllib.request.urlopen(urllib.request.Request(url, headers=AGENT), timeout=120) as r:
        return r.read()


def size_of(url):
    """The size of a release file from its download link (no API, so no rate limit); None while it isn't there."""
    req = urllib.request.Request(url, method="HEAD", headers={"User-Agent": AGENT["User-Agent"]})
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return int(r.headers.get("Content-Length") or 0)
    except urllib.error.HTTPError as err:
        if err.code == 404:
            return None
        raise


def release(tag):
    """The sizes of this version's release files once every one is on GitHub; None while it's still building."""
    base = f"https://github.com/{REPO}/releases/download/{tag}"
    sizes = {}
    for name in sorted(set(PLATFORMS.values()) | {"SHA256SUMS.txt", "Anti-Horizon-Launcher-mac.dmg"}):
        size = size_of(f"{base}/{name}")
        if size is None:
            return None
        sizes[name] = size
    return sizes


def main():
    latest_path = os.path.join(UPDATES, "latest.json")
    latest = json.load(open(latest_path, encoding="utf-8"))
    version = latest["version"]
    tag = f"v{version}"

    deadline = time.time() + 45 * 60
    rel = release(tag)
    while rel is None:
        if time.time() > deadline:
            sys.exit(f"GitHub hasn't finished {tag} yet. See https://github.com/{REPO}/actions")
        print(f"Waiting for GitHub to build {tag}...")
        time.sleep(60)
        rel = release(tag)

    base = f"https://github.com/{REPO}/releases/download/{tag}"
    sums = {}
    for line in get(f"{base}/SHA256SUMS.txt").decode().splitlines():
        digest, _, name = line.partition("  ")
        sums[name.strip().lstrip("*")] = digest.strip()

    shutil.rmtree(OUT, ignore_errors=True)
    os.makedirs(OUT)
    for name in sorted(set(PLATFORMS.values())):
        print(f"Downloading {name}")
        data = get(f"{base}/{name}")
        if hashlib.sha256(data).hexdigest() != sums.get(name):
            sys.exit(f"{name} doesn't match its SHA-256 in the release. Not signing it.")
        open(os.path.join(OUT, name), "wb").write(data)

    # Sign with the same key as the Windows installer. The key reaches the signer through its environment only.
    env = dict(os.environ)
    env["TAURI_SIGNING_PRIVATE_KEY"] = open(os.path.join(KEYS, "launcher-update.key"), encoding="utf-8").read().strip()
    env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = open(os.path.join(KEYS, "launcher-update.password"), encoding="utf-8").read().strip()
    npx = shutil.which("npx") or "npx"
    signatures = {}
    for name in sorted(set(PLATFORMS.values())):
        path = os.path.join(OUT, name)
        run = subprocess.run([npx, "--yes", "@tauri-apps/cli@2", "signer", "sign", path], env=env, capture_output=True, text=True)
        if run.returncode != 0 or not os.path.exists(path + ".sig"):
            sys.exit(f"Signing {name} failed.")
        signatures[name] = open(path + ".sig", encoding="utf-8").read().strip()
    del env

    # The download sizes, for the website.
    sizes = rel
    open(os.path.join(OUT, "sizes.json"), "w", encoding="utf-8").write(json.dumps(sizes, indent=2))

    for platform, name in PLATFORMS.items():
        latest["platforms"][platform] = {"signature": signatures[name], "url": f"{base}/{name}"}
    open(latest_path, "w", encoding="utf-8", newline="\n").write(json.dumps(latest, indent=2) + "\n")
    print(f"latest.json now updates Windows, Mac and Linux to {version}. Next: python site-build.py, then publish the site.")


if __name__ == "__main__":
    main()
