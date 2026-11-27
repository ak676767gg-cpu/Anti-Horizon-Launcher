"""
Builds the Minecraft 26.x half of the Anti-Horizon Client from the same source as the 1.21.11 half.

Minecraft 26.x renamed its drawing API (GuiGraphics -> GuiGraphicsExtractor, render -> extract...) and moved the
current screen onto Gui. This copies src/client/java/com/antihorizon/client into the package
com.antihorizon.client.v26, applies those renames, and compiles it with javac against the game and Fabric API jars the
Modrinth app has for 26.2 (then checks it against 26.3 too). 26.x isn't obfuscated, so the classes need no remapping.

    python tools/port26.py <out-classes-dir>
"""
import glob
import json
import os
import re
import shutil
import subprocess
import sys
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "src", "client", "java", "com", "antihorizon", "client")
WORK = os.path.join(ROOT, "build", "port26")
JAVAC = os.path.join(os.environ.get("JAVA_HOME") or os.path.join(os.path.expanduser("~"), "tools", "jdk-25.0.5.1+2"), "bin", "javac")
META = os.path.join(os.environ["APPDATA"], "ModrinthApp", "meta")

# Whole-word and text renames, in order.
RULES = [
    (r"^package com\.antihorizon\.client\.mixin;", "package com.antihorizon.client.v26.mixin;"),
    (r"^package com\.antihorizon\.client;", "package com.antihorizon.client.v26;"),
    (r"import com\.antihorizon\.client\.", "import com.antihorizon.client.v26."),
    (r"\bGuiGraphics\b", "GuiGraphicsExtractor"),
    (r"\.drawString\(", ".text("),
    (r"\.renderItem\(", ".item("),
    (r"public void renderBackground\(GuiGraphicsExtractor", "public void extractBackground(GuiGraphicsExtractor"),
    (r"super\.renderBackground\(", "super.extractBackground("),
    (r"public void render\(GuiGraphicsExtractor", "public void extractRenderState(GuiGraphicsExtractor"),
    (r"super\.render\(", "super.extractRenderState("),
    (r"\b(mc|minecraft|client)\.screen\b", r"\1.gui.screen()"),
    (r"\b(mc|minecraft|client)\.setScreen\(", r"\1.gui.setScreen("),
    (r"\bPlayerFaceRenderer\.draw\(", "PlayerFaceExtractor.extractRenderState("),
    (r"\bPlayerFaceRenderer\b", "PlayerFaceExtractor"),
    (r"new OptionsScreen\(this, minecraft\.options\)", "new OptionsScreen(this, minecraft.options, false)"),
    # Fabric only draws HUD elements while the HUD is showing, so F1 needs no check of our own.
    (r"mc\.options\.hideGui \|\| ", ""),
    # The HUD hook: Fabric API's HudElementRegistry replaces HudRenderCallback.
    (r"import net\.fabricmc\.fabric\.api\.client\.rendering\.v1\.HudRenderCallback;",
     "import net.fabricmc.fabric.api.client.rendering.v1.hud.HudElementRegistry;\nimport net.minecraft.resources.Identifier;"),
    (r"HudRenderCallback\.EVENT\.register\(\(graphics, tick\) -> Hud\.render\(graphics\)\);",
     'HudElementRegistry.addLast(Identifier.fromNamespaceAndPath("antihorizon", "hud"), (graphics, tick) -> Hud.render(graphics));'),
    # The title-screen swap hooks Gui.setScreen, where 26.x keeps the current screen.
    (r"import net\.minecraft\.client\.Minecraft;\n(?=import net\.minecraft\.client\.gui\.screens\.Screen;\nimport net\.minecraft\.client\.gui\.screens\.TitleScreen;)",
     "import net.minecraft.client.gui.Gui;\n"),
    (r"@Mixin\(Minecraft\.class\)", "@Mixin(Gui.class)"),
    # The panorama: PanoramaRenderer.render(g, w, h, fade) became Panorama.extractRenderState(g, w, h).
    (r"PanoramaRenderer", "Panorama"),
    (r'method = "render", at = @At\("HEAD"\), cancellable = true', 'method = "extractRenderState", at = @At("HEAD"), cancellable = true'),
    (r", int height, boolean fade, CallbackInfo ci\)", ", int height, CallbackInfo ci)"),
    # Fabric API's name for the render-layer event.
    (r"\bLivingEntityFeatureRendererRegistrationCallback\b", "LivingEntityRenderLayerRegistrationCallback"),
]


# 26.3 moved part of the renderer into com.mojang.renderpearl and replaced GLFW with SDL.
RULES_263 = [
    (r"com\.mojang\.blaze3d\.textures\.FilterMode", "com.mojang.renderpearl.api.textures.FilterMode"),
    (r"InputConstants\.Type\.KEYSYM", "InputConstants.Type.KEYBOARD"),
    (r"InputConstants\.isKeyDown\(mc\.getWindow\(\), ", "InputConstants.isKeyDown("),
    # 26.3 put the options screen back to two arguments, and opens links through Blaze3D.
    (r"new OptionsScreen\(this, minecraft\.options, false\)", "new OptionsScreen(this, minecraft.options)"),
    (r"Util\.getPlatform\(\)\.openUri\(", "com.mojang.blaze3d.Blaze3D.openUri("),
    # PoseStack.mulPose(rotation) became rotate(rotation).
    (r"\.mulPose\(Axis\.", ".rotate(Axis."),
]

# Each build: its package, the game version it's compiled against, other versions it must also compile for, extra rules.
BUILDS = [
    ("v26", "26.2", [], []),
    ("v263", "26.3", [], RULES_263),
]


def classpath(ver, api_dir):
    vdir = os.path.join(META, "versions", f"{ver}-0.20.1")
    vj = json.load(open(os.path.join(vdir, f"{ver}-0.20.1.json"), encoding="utf-8"))
    paths = [os.path.join(vdir, f"{ver}-0.20.1.jar")]
    for lib in vj["libraries"]:
        g, a, v = lib["name"].split(":")[:3]
        p = os.path.join(META, "libraries", *g.split("."), a, v, f"{a}-{v}.jar")
        if os.path.exists(p) and "intermediary" not in a:
            paths.append(p)
    # LWJGL modules the Modrinth app hasn't downloaded for this version: any 3.4.x copy has the same API.
    have = {os.path.basename(os.path.dirname(os.path.dirname(p))) for p in paths}
    for mod in ("lwjgl", "lwjgl-stb", "lwjgl-glfw"):
        if mod not in have:
            paths += sorted(p for p in glob.glob(os.path.join(META, "libraries", "org", "lwjgl", mod, "3.4.*", f"{mod}-3.4.*.jar")) if "natives" not in p)[-1:]
    apis = sorted(glob.glob(os.path.join(os.environ["APPDATA"], "ModrinthApp", "profiles", "*", "mods", f"fabric-api-*+{ver}.jar")))
    if not apis:
        sys.exit(f"No Fabric API for {ver} in the Modrinth app's profiles")
    os.makedirs(api_dir, exist_ok=True)
    with zipfile.ZipFile(apis[-1]) as z:
        for n in z.namelist():
            if n.startswith("META-INF/jars/") and n.endswith(".jar"):
                dst = os.path.join(api_dir, os.path.basename(n))
                if not os.path.exists(dst):
                    open(dst, "wb").write(z.read(n))
                paths.append(dst)
    return ";".join(paths)


def port(pkg, extra):
    src_out = os.path.join(WORK, pkg, "src")
    shutil.rmtree(src_out, ignore_errors=True)
    files = []
    for dirpath, _, names in os.walk(SRC):
        for n in names:
            if not n.endswith(".java"):
                continue
            rel = os.path.relpath(os.path.join(dirpath, n), SRC)
            text = open(os.path.join(dirpath, n), encoding="utf-8").read()
            for pat, rep in RULES + extra:
                text = re.sub(pat, rep, text, flags=re.M)
            text = text.replace("com.antihorizon.client.v26", "com.antihorizon.client." + pkg)
            dst = os.path.join(src_out, "com", "antihorizon", "client", pkg, rel)
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            open(dst, "w", encoding="utf-8", newline="\n").write(text)
            files.append(dst)
    return files


def compile_for(ver, files, out):
    cp = classpath(ver, os.path.join(WORK, f"fabric-api-{ver}"))
    shutil.rmtree(out, ignore_errors=True)
    os.makedirs(out)
    argfile = os.path.join(WORK, f"files-{ver}.txt")
    open(argfile, "w", encoding="utf-8").write("\n".join(f'"{f}"'.replace("\\", "/") for f in files))
    r = subprocess.run([JAVAC, "-proc:none", "-encoding", "UTF-8", "--release", "21", "-nowarn", "-Xmaxerrs", "200",
                        "-cp", cp, "-d", out, "@" + argfile], capture_output=True, text=True)
    return r.returncode, r.stderr


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(WORK, "classes")
    shutil.rmtree(out, ignore_errors=True)
    failed = False
    for pkg, ver, also, extra in BUILDS:
        files = port(pkg, extra)
        code, err = compile_for(ver, files, out + "-" + pkg)
        if code != 0:
            print(err)
            print(f"{pkg} ({ver}): doesn't compile")
            failed = True
            continue
        shutil.copytree(out + "-" + pkg, out, dirs_exist_ok=True)
        print(f"{pkg}: compiled {len(files)} files against {ver}")
    sys.exit(1 if failed else 0)
