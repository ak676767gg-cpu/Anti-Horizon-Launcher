"""
Makes the live clips from the Anti-Horizon game's title screen: its ten "sights", 15-second loops recorded from the
native game (PROJECT_ACCRETION/game/js/fx/backdrops/backdrop-<n>.mp4 and .jpg). Each is used two ways:

  launcher background   the game's own clip, unchanged (1280x720, 30 fps)   site-assets/clips/<id>.mp4
  cape                  its middle, tall around the black hole, 240x384,    site-assets/clips/cape-<id>.ahv
                        24 fps JPEG frames for the client mod

Neither ships inside the launcher or the client: both download them the first time they're used and check them
against the SHA-256 written here into:

  dist/clips.json                                     the launcher's list (and dist/img/clip-<id>.jpg, the still)
  cosmetics/clip-capes.json                           the cape entries cosmetics-art.py adds to the catalog
  textures/cosmetics/cape/clip-<id>-face.png          a still, shown until the clip has downloaded

    python clips/make-clips.py        (then python ../anti-horizon-client/tools/cosmetics-art.py)
"""
import glob
import hashlib
import io
import json
import math
import os
import shutil
import struct
import subprocess
import sys
import tempfile

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CLIENT = os.path.join(os.path.dirname(ROOT), "anti-horizon-client")
OUT = os.path.join(ROOT, "site-assets", "clips")
SIGHTS = os.path.join(os.path.expanduser("~"), "PROJECT_ACCRETION", "game", "js", "fx", "backdrops")

# The game's title-screen order and names (menu-live.js).
CLIPS = [
    ("galactic-core", "Galactic-core black hole"),
    ("emission-nebula", "Emission nebula"),
    ("stellar-black-hole", "Stellar-origin black hole"),
    ("interstallar", "The Interstallar"),
    ("molecular-cloud", "Molecular cloud"),
    ("massive-black-hole", "Massive black hole"),
    ("singularity", "Singularity"),
    ("galactic-nebula", "Galactic nebula"),
    ("intermediate-black-hole", "Intermediate-mass black hole"),
    ("ultramassive-black-hole", "Ultramassive black hole"),
]
CAPE_W, CAPE_H, CAPE_FPS = 240, 384, 24


def ffmpeg():
    if os.environ.get("FFMPEG"):
        return os.environ["FFMPEG"]
    found = glob.glob(os.path.join(os.environ.get("LOCALAPPDATA", ""), "npm-cache", "_npx", "*", "node_modules", "ffmpeg-static", "ffmpeg.exe"))
    if not found:
        sys.exit("No ffmpeg: set FFMPEG, or run `npx ffmpeg-static` once.")
    return found[0]


def run(args):
    r = subprocess.run([ffmpeg(), "-hide_banner", "-loglevel", "error", "-y", *args], capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(r.stderr)


def sha(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def centre(still):
    """Across the picture, where the cape should centre: on the black hole's shadow if there is one, else the middle."""
    small = still.convert("L").resize((160, 90), Image.BILINEAR)
    px = small.load()
    dark = {(x, y) for y in range(12, 78) for x in range(24, 136) if px[x, y] < 14}
    best, seen = [], set()
    for start in dark:
        if start in seen:
            continue
        group, todo = [], [start]
        seen.add(start)
        while todo:
            x, y = todo.pop()
            group.append((x, y))
            for n in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
                if n in dark and n not in seen:
                    seen.add(n)
                    todo.append(n)
        # Open sky touches the edge of the search area; a shadow is closed in by its bright ring.
        if any(x in (24, 135) or y in (12, 77) for x, y in group):
            continue
        if len(group) > len(best):
            best = group
    if len(best) < 20:
        return 0.5
    return (sum(p[0] for p in best) / len(best) + 0.5) / 160


def main():
    shutil.rmtree(OUT, ignore_errors=True)
    os.makedirs(OUT)
    for old in glob.glob(os.path.join(ROOT, "dist", "img", "*clip-*.jpg")):
        os.remove(old)
    launcher, capes = [], []
    for n, (cid, name) in enumerate(CLIPS):
        src_mp4 = os.path.join(SIGHTS, f"backdrop-{n}.mp4")
        still = Image.open(os.path.join(SIGHTS, f"backdrop-{n}.jpg")).convert("RGB")
        mp4 = os.path.join(OUT, f"{cid}.mp4")
        shutil.copy2(src_mp4, mp4)
        still.save(os.path.join(ROOT, "dist", "img", f"clip-{cid}.jpg"), quality=88)
        still.resize((320, 180), Image.LANCZOS).save(os.path.join(ROOT, "dist", "img", f"thumb-clip-{cid}.jpg"), quality=85)
        launcher.append({"id": cid, "name": name, "file": f"{cid}.mp4", "size": os.path.getsize(mp4), "sha256": sha(mp4)})

        # The cape: a tall (10:16) slice of the clip, centred on the hole, as JPEG frames in the client's AHV1 file.
        w, h = still.size
        cw = h * 10 / 16
        x = min(max(0.0, centre(still) * w - cw / 2), w - cw)
        with tempfile.TemporaryDirectory() as tmp:
            run(["-i", src_mp4, "-vf", f"crop={int(cw)}:{h}:{int(x)}:0,scale={CAPE_W}:{CAPE_H}:flags=lanczos,fps={CAPE_FPS}", "-q:v", "3", os.path.join(tmp, "f%04d.jpg")])
            frames = [open(p, "rb").read() for p in sorted(glob.glob(os.path.join(tmp, "f*.jpg")))]
        ahv = os.path.join(OUT, f"cape-{cid}.ahv")
        with open(ahv, "wb") as f:
            f.write(b"AHV1" + struct.pack(">iiii", len(frames), CAPE_FPS, CAPE_W, CAPE_H))
            f.write(b"".join(struct.pack(">i", len(fr)) for fr in frames))
            f.write(b"".join(frames))
        face = Image.open(io.BytesIO(frames[0])).convert("RGB")
        for base in (os.path.join(CLIENT, "src", "main", "resources", "assets", "antihorizon", "textures", "cosmetics", "cape"),
                     os.path.join(ROOT, "dist", "cosmetics", "cape")):
            os.makedirs(base, exist_ok=True)
            face.save(os.path.join(base, f"clip-{cid}-face.png"), optimize=True)
        capes.append({"id": f"clip-{cid}", "name": name, "clip": {
            "file": f"cape-{cid}.ahv", "size": os.path.getsize(ahv), "sha256": sha(ahv), "frames": len(frames), "fps": CAPE_FPS,
            # The launcher's preview plays the background clip on the cape: the same tall part of it.
            "video": cid, "crop": [round(x / w, 4), 0, round((x + cw) / w, 4), 1]}})
        print(f"  {cid}: {os.path.getsize(mp4) / 1048576:.1f} MB background, cape {len(frames)} frames {os.path.getsize(ahv) / 1048576:.1f} MB, centre {x / w + cw / w / 2:.2f}")

    open(os.path.join(ROOT, "dist", "clips.json"), "w", encoding="utf-8", newline="\n").write(json.dumps(launcher, indent=2) + "\n")
    open(os.path.join(ROOT, "cosmetics", "clip-capes.json"), "w", encoding="utf-8", newline="\n").write(json.dumps(capes, indent=2) + "\n")


if __name__ == "__main__":
    main()
