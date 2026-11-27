# Makes a launcher release: raises the version, builds the signed NSIS installer, and writes the two files installed
# launchers update from into release\updates:
#   latest.json                       the newest version, its notes, and the installer's signature
#   Anti-Horizon-Launcher-Setup.exe   the installer itself (also copied to release\Anti-Horizon Setup.exe)
# Nothing is published. Going live means putting both files at https://anti-horizon.com/launcher/ (the game repo's
# site\launcher folder) and publishing the website.
#
#   powershell -ExecutionPolicy Bypass -File release.ps1                      next patch version (0.2.0 -> 0.2.1)
#   powershell -ExecutionPolicy Bypass -File release.ps1 -Version 0.3.0 -Notes "Console, test mode"
#
# The signing key lives outside the repo in %USERPROFILE%\.anti-horizon. Installed launchers only accept installers
# signed with it, so if it is lost they can't update any more and everyone has to reinstall once.
param(
  [string]$Version = "",
  [string]$Notes = "",
  [string]$Base = "https://anti-horizon.com/launcher",
  [switch]$AllowHttp # test builds only: lets the updater fetch from a local http server
)
$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$keyDir = Join-Path $env:USERPROFILE ".anti-horizon"
$key = Join-Path $keyDir "launcher-update.key"
$password = Join-Path $keyDir "launcher-update.password"
if (-not (Test-Path $key) -or -not (Test-Path $password)) { throw "The update signing key isn't in $keyDir." }

function Write-Utf8([string]$path, [string]$text) {
  [IO.File]::WriteAllText($path, $text, (New-Object Text.UTF8Encoding($false)))
}

# The version, in both places Tauri reads it.
$confPath = Join-Path $root "src-tauri\tauri.conf.json"
$cargoPath = Join-Path $root "src-tauri\Cargo.toml"
$conf = [IO.File]::ReadAllText($confPath)
$old = ([regex]'"version":\s*"([^"]+)"').Match($conf).Groups[1].Value
if (-not $Version) {
  $parts = $old.Split(".")
  $parts[2] = [string]([int]$parts[2] + 1)
  $Version = $parts -join "."
}
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Versions look like 1.2.3, not '$Version'." }
Write-Utf8 $confPath (([regex]'"version":\s*"[^"]+"').Replace($conf, "`"version`": `"$Version`"", 1))
$cargo = [IO.File]::ReadAllText($cargoPath)
Write-Utf8 $cargoPath (([regex]'(?m)^version = "[^"]+"').Replace($cargo, "version = `"$Version`"", 1))
"Version $old -> $Version"

# Build and sign. The keys are handed to the build through its environment only.
$env:TAURI_SIGNING_PRIVATE_KEY = [IO.File]::ReadAllText($key).Trim()
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = [IO.File]::ReadAllText($password).Trim()
# The launcher's CurseForge API key, built into the app. Without the file, the release has Modrinth only.
$cfKey = Join-Path $keyDir "curseforge-key.txt"
if (Test-Path $cfKey) {
  $env:AH_CURSEFORGE_KEY = [IO.File]::ReadAllText($cfKey).Trim()
  "CurseForge: on (key from $cfKey)"
} else {
  Remove-Item Env:AH_CURSEFORGE_KEY -ErrorAction SilentlyContinue
  "CurseForge: off (no $cfKey yet)"
}
# The Microsoft sign-in app ID, built in the same way: it isn't in the source, so builds of the public code can't use it.
$msa = Join-Path $keyDir "msa-client-id.txt"
if (-not (Test-Path $msa)) { throw "The Microsoft sign-in ID isn't in $msa." }
$env:AH_MSA_CLIENT_ID = [IO.File]::ReadAllText($msa).Trim()
# The Anti-Horizon client that presets install ships inside the launcher: take the newest build of the client project.
$clientJar = Join-Path $env:USERPROFILE "anti-horizon-client\build\libs\anti-horizon-client.jar"
if (Test-Path $clientJar) {
  Copy-Item $clientJar (Join-Path $root "src-tauri\presets\anti-horizon-client.jar") -Force
  "Client: $clientJar"
} else {
  "Client: no build at $clientJar, keeping src-tauri\presets\anti-horizon-client.jar"
}
$extra = @()
if ($AllowHttp) {
  $cfg = Join-Path $env:TEMP "ah-launcher-allow-http.json"
  Write-Utf8 $cfg '{"plugins":{"updater":{"dangerousInsecureTransportProtocol":true}}}'
  $extra = @("--config", $cfg)
  "Test build: this one may update from plain http."
}
Push-Location $root
try {
  & npx --yes "@tauri-apps/cli@2" build @extra
  if ($LASTEXITCODE -ne 0) { throw "The build failed." }
} finally {
  Pop-Location
  Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY, Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD, Env:AH_CURSEFORGE_KEY, Env:AH_MSA_CLIENT_ID -ErrorAction SilentlyContinue
}

$setup = Get-ChildItem (Join-Path $root "src-tauri\target\release\bundle\nsis") -Filter "*_${Version}_x64-setup.exe" | Select-Object -First 1
if (-not $setup) { throw "The build didn't make an installer for $Version." }
$sigFile = "$($setup.FullName).sig"
if (-not (Test-Path $sigFile)) { throw "The installer wasn't signed (no .sig file)." }

$out = Join-Path $root "release\updates"
New-Item -ItemType Directory -Force $out | Out-Null
$name = "Anti-Horizon-Launcher-Setup.exe"
Copy-Item $setup.FullName (Join-Path $out $name) -Force
Copy-Item $setup.FullName (Join-Path $root "release\Anti-Horizon Setup.exe") -Force
$latest = [ordered]@{
  version   = $Version
  notes     = $Notes
  pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  platforms = [ordered]@{
    "windows-x86_64" = [ordered]@{
      signature = [IO.File]::ReadAllText($sigFile).Trim()
      url       = "$($Base.TrimEnd('/'))/$name"
    }
  }
}
Write-Utf8 (Join-Path $out "latest.json") ($latest | ConvertTo-Json -Depth 5)

$mb = [math]::Round((Get-Item (Join-Path $out $name)).Length / 1MB, 1)
if ($mb -gt 24) { Write-Warning "The installer is $mb MB; Cloudflare Pages only serves files up to 25 MB." }
"Done: v$Version ($mb MB) in $out"
"Not published. To ship it, copy both files into the website's launcher folder and publish the site."
