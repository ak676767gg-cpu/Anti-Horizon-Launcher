"""
Draws the Anti-Horizon cosmetics' textures and writes them where the client and the launcher read them:

  capes    the launcher's black holes, cropped tall around the hole: a sharp face (320x512, and 160x256 for far away)
           that the client turns around the hole, plus Minecraft's cape layout (64x32 at 8x) for the inside, the
           edges and elytra; where each hole is goes in the catalog
  wings    layered feathers, drawn for the right wing (the left one mirrors it)
  head     the halo's gold, the accretion ring and the crown

    python tools/cosmetics-art.py

Client:   src/main/resources/assets/antihorizon/textures/cosmetics/<kind>/<id>.png
Launcher: ../anti-horizon/dist/cosmetics/<kind>/<id>.png (its 3D preview), plus the catalog both read.
"""
import json
import math
import os
import random

from PIL import Image, ImageChops, ImageDraw, ImageEnhance, ImageFilter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LAUNCHER = os.path.join(os.path.dirname(ROOT), "anti-horizon")
ART = os.path.join(LAUNCHER, "dist", "img")
OUT_CLIENT = os.path.join(ROOT, "src", "main", "resources", "assets", "antihorizon", "textures", "cosmetics")
OUT_LAUNCHER = os.path.join(LAUNCHER, "dist", "cosmetics")
CATALOG = os.path.join(LAUNCHER, "cosmetics", "catalog.json")
# Live capes made from the recordings by the launcher's clips/make-clips.py: their stills are already in place.
CLIP_CAPES = os.path.join(LAUNCHER, "cosmetics", "clip-capes.json")

GOLD = (216, 188, 141)
GOLD_DEEP = (138, 106, 58)
IVORY = (243, 230, 200)
VOID = (7, 14, 21)
S = 8  # pixels per cape texel

# The game's own black holes and nebulae are live capes now (clips/make-clips.py, from its title screen); these two
# aren't among them, so they stay as pictures whose disk turns.
CAPES = [
    # id, name, background, where the hole is (fraction of the image), how much of the image's height to show,
    # and how its disk lies: tilt (degrees, clockwise), how flat it looks (1 = seen face on), which way it turns
    ("crimson", "Crimson", "crimson", (0.54, 0.50), 0.85, 0, 0.85, 1),
    ("violet", "Violet", "violet", (0.52, 0.50), 0.85, 0, 0.80, 1),
]
WINGS = [("angel", "Angel Wings"), ("gold", "Gold Wings"), ("void", "Void Wings")]
HEAD = [("halo", "Halo"), ("accretion", "Accretion Ring")]
FACE = (320, 512)        # the outside of the cape, sharp up close
FACE_FAR = (160, 256)    # the same for capes further than six blocks away (less shimmer)


def save(img, kind, name):
    for base in ((OUT_LAUNCHER,) if kind == "thumb" else (OUT_CLIENT, OUT_LAUNCHER)):
        path = os.path.join(base, kind, name + ".png")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        img.save(path, optimize=True)


def mix(a, b, t):
    t = max(0.0, min(1.0, t))
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def crop(im, centre, height_frac, aspect):
    """A crop of `aspect` (w/h) around `centre`, `height_frac` of the image tall, kept inside the image."""
    w, h = im.size
    ch = min(h, int(h * height_frac))
    cw = int(ch * aspect)
    if cw > w:
        cw, ch = w, int(w / aspect)
    x = min(max(0, int(centre[0] * w - cw / 2)), w - cw)
    y = min(max(0, int(centre[1] * h - ch / 2)), h - ch)
    return im.crop((x, y, x + cw, y + ch))


def put(tex, im, x, y, w, h):
    """Fills cape texels (x, y, w, h) with the image."""
    tex.paste(im.resize((w * S, h * S), Image.LANCZOS), (x * S, y * S))


def find_hole(face):
    """
    Where the black hole is on the cape's face and how big: the largest patch of near-black pixels in the middle of
    the picture (the shadow), as (x, y, radius), all as fractions of the face's width.
    """
    small = face.convert("L").resize((80, 128), Image.BILINEAR)
    w, h = small.size
    px = small.load()
    dark = {(x, y) for y in range(int(h * 0.12), int(h * 0.88)) for x in range(int(w * 0.12), int(w * 0.88)) if px[x, y] < 22}
    best = []
    seen = set()
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
        # Open sky touches the edge of the search area; the shadow is closed in by the bright ring around it.
        x0, x1, y0, y1 = int(w * 0.12), int(w * 0.88) - 1, int(h * 0.12), int(h * 0.88) - 1
        if any(x in (x0, x1) or y in (y0, y1) for x, y in group):
            continue
        if len(group) > len(best):
            best = group
    if not best:
        return 0.5, 0.5, 0.12
    cx = sum(p[0] for p in best) / len(best) + 0.5
    cy = sum(p[1] for p in best) / len(best) + 0.5
    r = math.sqrt(len(best) / math.pi)
    return cx / w, cy / h, r / w


def cape(cid, background, centre, height_frac, tilt, ratio, spin):
    src = Image.open(os.path.join(ART, f"bg-{background}.jpg")).convert("RGB")
    front = crop(src, centre, height_frac, 10 / 16)
    face = front.resize(FACE, Image.LANCZOS)
    save(face, "cape", cid + "-face")
    save(front.resize(FACE_FAR, Image.LANCZOS), "cape", cid + "-face-far")

    # Minecraft's cape layout (64x32 texels): used for the inside, the edges and elytra. No outline anywhere.
    tex = Image.new("RGBA", (64 * S, 32 * S), (0, 0, 0, 0))
    inside = ImageEnhance.Brightness(front.transpose(Image.FLIP_LEFT_RIGHT)).enhance(0.35)
    put(tex, front, 1, 1, 10, 16)
    put(tex, inside, 12, 1, 10, 16)
    # The edges: the picture's own colours along its border, darkened, so they read as the cloth's thickness.
    edge = ImageEnhance.Brightness(front).enhance(0.45)
    put(tex, edge.crop((0, 0, 8, front.height)), 0, 1, 1, 16)
    put(tex, edge.crop((front.width - 8, 0, front.width, front.height)), 11, 1, 1, 16)
    put(tex, edge.crop((0, 0, front.width, 8)), 1, 0, 10, 1)
    put(tex, edge.crop((0, front.height - 8, front.width, front.height)), 11, 0, 10, 1)
    # Elytra (22,0)-(46,22): the outer face is (24,2)-(34,22); give it the hole and the rest the sky.
    wide = crop(src, centre, min(1.0, height_frac * 1.1), 24 / 22)
    put(tex, ImageEnhance.Brightness(wide).enhance(0.6), 22, 0, 24, 22)
    put(tex, crop(src, centre, height_frac, 10 / 20), 24, 2, 10, 20)
    save(tex, "cape", cid)

    x, y, r = find_hole(face)
    # The disk turns around the hole: hole = where nothing moves; outer = where the motion has faded out (both in
    # face widths, measured in the disk's own plane).
    # mix: how much of the disk is the turning copy. A disk seen face on turns whole; a flat, side-on one keeps some
    # of its still picture under the turning light, since turning a side-on picture pulls in the sky above it.
    return {"x": round(x, 4), "y": round(y, 4), "hole": round(r * 1.08, 4), "outer": round(max(0.55, r * 5.5), 4),
            "tilt": tilt, "ratio": ratio, "spin": spin, "mix": 1.0 if ratio >= 0.6 else 0.65}


# ---------------------------------------------------------------- wings

# Jointed wings in Minecraft's own pixel style (after Fantastic Wings): three pieces per wing, shoulder to hand, each
# a sheet of feathers hanging from its bone, at 2 texels per skin pixel. One 64x64 atlas per design:
#   piece        skin pixels (wide x tall)   atlas texels (x, y, w, h)
#   1 shoulder   6 x 12                      (0, 0, 12, 24)
#   2 forearm    8 x 15                      (12, 0, 16, 30)
#   3 hand       11 x 18                     (28, 0, 22, 36)
# The client (CosmeticsLayer) and the launcher's preview (preview3d.js) read the same numbers.
WING_PIECES = [(0, 12, 24), (12, 16, 30), (28, 22, 36)]

WING_PALETTES = {
    # highlight, light, mid, shadow, deep, outline, (edge and star colours for void)
    "angel": [(255, 255, 255), (250, 251, 253), (238, 241, 247), (219, 224, 235), (196, 203, 218), (160, 168, 188)],
    "gold": [(255, 238, 186), (244, 213, 148), (222, 180, 106), (188, 140, 70), (150, 104, 48), (104, 70, 32)],
    "void": [(78, 80, 132), (56, 58, 108), (40, 40, 82), (28, 28, 60), (18, 18, 40), (216, 188, 141)],
}


def pixel_piece(w, h, piece, pal, rng, style):
    """One wing piece: rounded coverts along the bone at the top, then feathers 3 texels wide hanging down."""
    hi, li, mid, sh, deep, line = pal
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    px = img.load()
    fw = 3
    n = w // fw
    covert = 6 if piece < 2 else 7
    for i in range(n):
        x0 = i * fw
        # Feather length: the trailing edge gets longer toward the wing tip, ragged like real feathers.
        t = (i + 0.5) / n
        if piece == 0:
            length = int(h * (0.72 + 0.22 * t))
        elif piece == 1:
            length = int(h * (0.80 + 0.18 * t))
        else:
            length = int(h * (0.98 - 0.5 * max(0.0, t - 0.55) ** 1.6 * 2.2))  # the hand's tip rounds off
        length = max(covert + 3, min(h, length - rng.choice((0, 0, 1, 2))))
        for y in range(covert - 1, length):
            f = (y - covert) / max(1, length - covert)
            for dx in range(fw):
                x = x0 + dx
                if x >= w:
                    continue
                # Shade down the feather, with a dither between bands, darker on its left edge, the shaft lighter.
                band = f * 3.2 + (0.5 if (x + y) % 2 else 0)
                col = [li, mid, sh, deep][min(3, int(band))]
                if dx == 0:
                    col = tuple(max(0, c - 22) for c in col) if style != "void" else deep
                if dx == 1 and f < 0.85:
                    col = tuple(min(255, c + 14) for c in col)
                px[x, y] = col + (255,)
        # The rounded tip: the last row only in the middle, and an outline under it.
        if 0 <= length - 1 < h:
            px[x0, length - 1] = (0, 0, 0, 0)
            if x0 + 2 < w:
                px[x0 + 2, length - 1] = (0, 0, 0, 0)
        if style == "void" and length - 2 > covert:
            for dx in range(fw):
                if x0 + dx < w:
                    px[x0 + dx, length - 2] = line + (255,)
    # Coverts: rows of rounded little feathers over the tops of the long ones, lightest at the bone.
    for row in range(2):
        yb = row * 3
        for x in range(w):
            k = (x + row * 2) % 4
            for y in range(yb, yb + 3 + (1 if k in (1, 2) else 0)):
                if y >= h:
                    continue
                col = hi if row == 0 and y == yb else (li if row == 0 else mid)
                if k == 0:
                    col = tuple(max(0, c - 26) for c in col)
                px[x, y] = col + (255,)
    # The bone: a clean edge along the top.
    for x in range(w):
        px[x, 0] = (line if style == "void" else hi) + (255,)
    if style == "void":
        a = img.getchannel("A")
        for _ in range(int(w * h / 26)):
            x, y = rng.randrange(w), rng.randrange(covert, h)
            if a.getpixel((x, y)) == 255:
                b = rng.choice((200, 230, 255))
                px[x, y] = (b, b, min(255, b + 20), 255)
    return img


def wing(style):
    rng = random.Random({"angel": 5, "gold": 9, "void": 13}[style])
    atlas = Image.new("RGBA", (64, 64), (0, 0, 0, 0))
    for piece, (ax, w, h) in enumerate(WING_PIECES):
        atlas.paste(pixel_piece(w, h, piece, WING_PALETTES[style], rng, style), (ax, 0))
    return atlas


# ---------------------------------------------------------------- head

def halo():
    img = Image.new("RGBA", (64, 16), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    for y in range(16):
        t = abs(y - 7.5) / 7.5
        # All white: bright in the middle of the band, a soft cool white at its edges.
        c = mix((255, 255, 255), (198, 206, 222), t ** 1.4)
        d.line([(0, y), (63, y)], fill=c + (255,))
    return img


def accretion():
    """A disk seen from above: white-hot at the inside, gold, then fading out, in streaks around the centre."""
    N = 512
    img = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    px = img.load()
    rng = random.Random(3)
    streak = [rng.random() for _ in range(720)]
    for y in range(N):
        for x in range(N):
            dx, dy = (x - N / 2) / (N / 2), (y - N / 2) / (N / 2)
            r = math.hypot(dx, dy)
            if r < 0.62 or r > 1:
                continue
            t = (r - 0.62) / 0.38
            a = math.atan2(dy, dx)
            s = streak[int((a + math.pi) / (2 * math.pi) * 719 + r * 140) % 720]
            c = mix((255, 248, 230), GOLD, min(1, t * 1.6)) if t < 0.6 else mix(GOLD, GOLD_DEEP, (t - 0.6) / 0.4)
            alpha = (1 - t) ** 1.4 * (0.65 + 0.35 * s)
            if t < 0.05:
                alpha *= t / 0.05
            px[x, y] = c + (int(255 * max(0, min(1, alpha))),)
    return img.filter(ImageFilter.GaussianBlur(0.8))


def crown():
    """Gold with a little shine on a 32x16 sheet: the left half metal, the right half gems (red, teal, violet)."""
    img = Image.new("RGBA", (32, 16), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    for y in range(16):
        c = mix((255, 236, 180), GOLD_DEEP, y / 15)
        d.line([(0, y), (15, y)], fill=c + (255,))
    for i, c in enumerate(((200, 40, 60), (40, 180, 170), (140, 80, 220))):
        x = 16 + i * 5
        d.rectangle((x, 0, x + 4, 15), fill=c + (255,))
        d.rectangle((x + 1, 1, x + 2, 3), fill=(255, 255, 255, 255))
    return img


def clip_cape(cid):
    """A live cape's still parts: the far face and Minecraft's cape layout (inside, edges, elytra) from its still."""
    face = Image.open(os.path.join(OUT_LAUNCHER, "cape", cid + "-face.png")).convert("RGB")
    save(face.resize(FACE_FAR, Image.LANCZOS), "cape", cid + "-face-far")
    tex = Image.new("RGBA", (64 * S, 32 * S), (0, 0, 0, 0))
    put(tex, face, 1, 1, 10, 16)
    put(tex, ImageEnhance.Brightness(face.transpose(Image.FLIP_LEFT_RIGHT)).enhance(0.35), 12, 1, 10, 16)
    edge = ImageEnhance.Brightness(face).enhance(0.45)
    put(tex, edge.crop((0, 0, 4, face.height)), 0, 1, 1, 16)
    put(tex, edge.crop((face.width - 4, 0, face.width, face.height)), 11, 1, 1, 16)
    put(tex, edge.crop((0, 0, face.width, 4)), 1, 0, 10, 1)
    put(tex, edge.crop((0, face.height - 4, face.width, face.height)), 11, 0, 10, 1)
    put(tex, ImageEnhance.Brightness(face).enhance(0.6), 22, 0, 24, 22)
    put(tex, face, 24, 2, 10, 20)
    save(tex, "cape", cid)


def main():
    flows = {}
    for cid, _, bg, centre, frac, tilt, ratio, spin in CAPES:
        flows[cid] = cape(cid, bg, centre, frac, tilt, ratio, spin)
    for wid, _ in WINGS:
        w = wing(wid)
        save(w, "wings", wid)
    save(halo(), "head", "halo")
    acc = accretion()
    save(acc, "head", "accretion")
    clips = json.load(open(CLIP_CAPES, encoding="utf-8")) if os.path.exists(CLIP_CAPES) else []
    for c in clips:
        clip_cape(c["id"])
    catalog = {
        "version": 1,
        "cape": [{"id": c[0], "name": c[1], "flow": flows[c[0]]} for c in CAPES] + clips,
        "wings": [{"id": w[0], "name": w[1]} for w in WINGS],
        "head": [{"id": h[0], "name": h[1]} for h in HEAD],
    }
    os.makedirs(os.path.dirname(CATALOG), exist_ok=True)
    text = json.dumps(catalog, indent=2) + "\n"
    for path in (CATALOG, os.path.join(OUT_CLIENT, "catalog.json"), os.path.join(OUT_LAUNCHER, "catalog.json")):
        open(path, "w", encoding="utf-8", newline="\n").write(text)
    print("cosmetics drawn:", len(CAPES), "capes,", len(WINGS), "wings,", len(HEAD), "head")


if __name__ == "__main__":
    main()
