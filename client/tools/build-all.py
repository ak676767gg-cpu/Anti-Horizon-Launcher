"""
Builds the one Anti-Horizon Client jar for every supported Minecraft version (1.21.11, 26.1, 26.2, 26.3).

  1. gradlew build      -> the 1.21.11 build and the start-up code (remapped by Loom), with all the resources
  2. tools/port26.py    -> the 26.1-26.2 build (com.antihorizon.client.v26) and the 26.3 build (...v263)
  3. both are packed into build/libs/anti-horizon-client.jar

    python tools/build-all.py
"""
import os
import subprocess
import sys
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# Java 25: JAVA_HOME, or a JDK unpacked in <your home folder>/tools.
JAVA_HOME = os.environ.get("JAVA_HOME") or os.path.join(os.path.expanduser("~"), "tools", "jdk-25.0.5.1+2")
LOOM_JAR = os.path.join(ROOT, "build", "libs", "anti-horizon-client-0.1.0.jar")
PORT = os.path.join(ROOT, "build", "port26", "classes")
OUT = os.path.join(ROOT, "build", "libs", "anti-horizon-client.jar")

env = dict(os.environ, JAVA_HOME=JAVA_HOME)
env.pop("NoDefaultCurrentDirectoryInExePath", None)
r = subprocess.run([os.path.join(ROOT, "gradlew.bat"), "build", "--console=plain", "-q"], cwd=ROOT, env=env)
if r.returncode != 0:
    sys.exit("gradle build failed")
r = subprocess.run([sys.executable, os.path.join(ROOT, "tools", "port26.py"), PORT], cwd=ROOT, env=env)
if r.returncode != 0:
    sys.exit("26.x build failed")

added = 0
with zipfile.ZipFile(LOOM_JAR) as zi, zipfile.ZipFile(OUT, "w", zipfile.ZIP_DEFLATED) as zo:
    for info in zi.infolist():
        zo.writestr(info, zi.read(info.filename))
    for dirpath, _, names in os.walk(PORT):
        for n in names:
            full = os.path.join(dirpath, n)
            arc = os.path.relpath(full, PORT).replace(os.sep, "/")
            zo.write(full, arc)
            added += 1
print(f"{OUT}: 1.21.11 + {added} classes for 26.x, {os.path.getsize(OUT) / 1048576:.1f} MB")
