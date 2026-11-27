"""Writes the site's plain pages (privacy, terms, contact, not found) with the home page's header and footer."""
import os

SITE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "site")
GH = "https://github.com/ak676767gg-cpu/Anti-Horizon-Launcher"

HEAD = """<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>{title} · Anti-Horizon Launcher</title>
  <meta name="description" content="{desc}">
  <link rel="canonical" href="https://anti-horizon.com{path}">
  <meta name="theme-color" content="#000000">
  <link rel="icon" href="/favicon.ico" sizes="48x48">
  <link rel="icon" href="/favicon.svg" type="image/svg+xml">
  <link rel="stylesheet" href="/home.css">
  <link rel="stylesheet" href="/doc.css">
</head>
<body>
  <header class="nav solid">
    <a class="brand" href="/"><img src="/logo.svg" width="24" height="24" alt=""><span>ANTI-HORIZON</span></a>
    <nav aria-label="Site"><a href="/">Launcher</a><a href="/privacy/">Privacy</a><a href="/terms/">Terms</a><a href="/contact/">Contact</a></nav>
    <a class="btn btn-mint btn-sm" href="/#download">Download</a>
  </header>
  <main class="doc" id="main">
"""

FOOT = """  </main>
  <footer class="foot">
    <a class="brand" href="/"><img src="/logo.svg" width="22" height="22" alt=""><span>ANTI-HORIZON</span></a>
    <nav aria-label="Pages"><a href="/">Home</a><a href="/privacy/">Privacy</a><a href="/terms/">Terms</a><a href="/contact/">Contact</a><a href="{gh}">Source code</a></nav>
    <p>A launcher by AK, an independent developer in Australia. © 2026.</p>
    <p class="legal">Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.</p>
  </footer>
</body>
</html>
""".replace("{gh}", GH)

PAGES = {
    "privacy/index.html": ("Privacy", "What the Anti-Horizon Launcher, its client and this website do with your information: almost nothing.", "/privacy/", f"""
    <p class="eyebrow">Last updated 27 November 2026</p>
    <h1>Privacy</h1>
    <p class="lede">The short version: we don't collect anything about you. There are no accounts with us, no analytics, no ads and no tracking, in the launcher, the client or on this website.</p>

    <h2>The launcher</h2>
    <ul>
      <li><b>Signing in.</b> You sign in on Microsoft's own page with a short code. The launcher never sees your password.</li>
      <li><b>Your sign-in afterwards.</b> The tokens Microsoft and Minecraft give back stay on your computer: on Windows they're encrypted for your Windows user, and on a Mac or Linux they're in a file only your user can open. They go only to Microsoft, Xbox and Minecraft's sign-in services, and to Minecraft when it starts.</li>
      <li><b>Who it talks to.</b> Microsoft, Xbox and Minecraft services (sign-in and your skin), Mojang (the game and its files), Fabric, Quilt, Forge and NeoForge (mod loaders), Modrinth and CurseForge (mods), this website (launcher updates), on a Mac or Linux GitHub (where those updates are kept), and Anti-Horizon's cosmetics service. Those services have their own privacy policies.</li>
      <li><b>Cosmetics.</b> When you pick capes, wings or head pieces, the launcher signs you in to Anti-Horizon's cosmetics service the way a Minecraft server checks a player: Mojang confirms the account is yours, so the service never sees your password or your Minecraft sign-in. It keeps your Minecraft player ID, the cosmetics you picked and when you last changed them, so other players can see them. Choose None for all three and that record is deleted.</li>
      <li><b>Your files.</b> Instances, worlds, mods, skin packs and settings stay in the launcher's own folder on your computer. Nothing is uploaded, except a skin you choose to save to your Minecraft account.</li>
    </ul>

    <h2>The Anti-Horizon client</h2>
    <p>The in-game mod downloads one file from this website, the background behind the title screen, once. To show cosmetics, it asks Anti-Horizon's cosmetics service which ones the players around you wear, sending only those players' IDs: nothing about you, your world, your chat or the server you're on. Switch Cosmetics off in its menu and it stops asking. It reads your username only to show it in its menu.</p>

    <h2>This website</h2>
    <p>No cookies, no analytics and no ads. It's hosted on Cloudflare, which, like any web host, handles the network requests your browser makes (such as your IP address) to deliver pages and keep the site safe. See <a href="https://www.cloudflare.com/privacypolicy/">Cloudflare's privacy policy</a>.</p>

    <h2>Children</h2>
    <p>We don't collect information from anyone, of any age. Minecraft accounts are Microsoft's, under Microsoft's rules.</p>

    <h2>Changes and questions</h2>
    <p>If this ever changes, this page will say so first, with a new date at the top. Questions: see <a href="/contact/">Contact</a>. You can also check all of the above yourself: <a href="{GH}">the code is public</a>.</p>
"""),
    "terms/index.html": ("Terms", "The terms for using the Anti-Horizon Launcher and the Anti-Horizon client.", "/terms/", f"""
    <p class="eyebrow">Last updated 27 November 2026</p>
    <h1>Terms</h1>
    <p class="lede">The Anti-Horizon Launcher and the Anti-Horizon client are free. By downloading or using them you agree to these terms.</p>

    <h2>Using them</h2>
    <ul>
      <li>You may download, install and use the launcher and the client for free, for anything noncommercial.</li>
      <li>Their code is published under the <a href="{GH}/blob/main/LICENSE">PolyForm Strict License 1.0.0</a>: you may read it and build it, but not copy it into other projects, change it, or share or re-upload the launcher or the client. Only download them from this website or the official GitHub repository.</li>
      <li>To play, you need your own copy of Minecraft: Java Edition. Minecraft, your Microsoft account and the game itself are covered by Microsoft's and Mojang's own terms, including the <a href="https://www.minecraft.net/eula">Minecraft EULA</a>.</li>
      <li>Mods, resource packs, shaders and modpacks you install belong to their authors and come under their own licenses.</li>
    </ul>

    <h2>No guarantees</h2>
    <p>The launcher and the client are provided as they are, without warranties of any kind. Back up worlds you care about. To the extent the law allows, AK is not liable for any loss or damage from using them, and any liability is limited to AU$10. Nothing here limits rights you have under the Australian Consumer Law that can't be excluded.</p>

    <h2>Not official</h2>
    <p>Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft. Minecraft is a trademark of Mojang Synergies AB.</p>

    <h2>Changes and law</h2>
    <p>These terms may change; the date at the top shows when they last did. They're governed by the laws of New South Wales, Australia.</p>
"""),
    "contact/index.html": ("Contact", "How to reach Anti-Horizon about the launcher and the client.", "/contact/", f"""
    <h1>Contact</h1>
    <p class="lede">The fastest way to reach us is on GitHub, where the launcher and the client live.</p>
    <div class="cards">
      <a class="card" href="{GH}/issues/new"><h2>Bugs and ideas</h2><p>Something not working, or an idea for the launcher or the client? Open an issue on GitHub.</p></a>
      <a class="card" href="{GH}/security/advisories/new"><h2>Security problems</h2><p>Found something that could put players or their accounts at risk? Report it privately here, not in a public issue.</p></a>
      <a class="card" href="{GH}"><h2>Read the code</h2><p>Want to check exactly what the launcher or the client does? All of it is public.</p></a>
    </div>
"""),
    "404.html": ("Not found", "This page doesn't exist.", "/404", """
    <h1>Not found</h1>
    <p class="lede">There's nothing at this address. The Anti-Horizon Launcher is on the <a href="/">home page</a>.</p>
"""),
}

for path, (title, desc, url, body) in PAGES.items():
    out = os.path.join(SITE, path)
    os.makedirs(os.path.dirname(out), exist_ok=True)
    html = HEAD.format(title=title, desc=desc, path=url) + body + FOOT
    if path == "404.html":
        html = html.replace('<link rel="canonical" href="https://anti-horizon.com/404">\n', '<meta name="robots" content="noindex">\n')
    open(out, "w", encoding="utf-8", newline="\n").write(html)
print("wrote", len(PAGES), "pages")
