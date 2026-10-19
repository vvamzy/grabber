# Registers the Download Manager native-messaging host with Chromium + Firefox.
# Requires the dm-host.exe path (defaults to the release build). Run elevated
# for HKLM (all users) or plain for HKCU (current user only).
#
# Usage:
#   .\register-host.ps1                 # current user, release dm-host.exe
#   .\register-host.ps1 -HostPath "C:\Program Files\Download Manager\dm-host.exe" -AllUsers
param(
    [string]$HostPath = (Join-Path $PSScriptRoot "target\release\dm-host.exe"),
    [string]$ExtensionId = "513cfb12c762a5b00468bc24efec0e2d",
    [switch]$AllUsers
)

$ErrorActionPreference = "Stop"
$HostPath = (Resolve-Path $HostPath).Path
$HOST_NAME = "com.downloadmanager.host"

# Where the native-host manifest JSON lives (stable, next to the exe).
$manifestDir = Split-Path $HostPath -Parent
$chromeManifest = Join-Path $manifestDir "$HOST_NAME.chrome.json"
$firefoxManifest = Join-Path $manifestDir "$HOST_NAME.firefox.json"

# Chrome/Edge native host manifest (allowed_origins uses the extension ID).
$chrome = @{
    name = $HOST_NAME
    description = "Download Manager native host"
    path = $HostPath
    type = "stdio"
    allowed_origins = @("chrome-extension://$ExtensionId/")
} | ConvertTo-Json
Set-Content -Path $chromeManifest -Value $chrome -Encoding ascii
Write-Host "wrote $chromeManifest"

# Firefox native host manifest (allowed_extensions uses the Gecko ID).
$firefox = @{
    name = $HOST_NAME
    description = "Download Manager native host"
    path = $HostPath
    type = "stdio"
    allowed_extensions = @("download-manager@local")
} | ConvertTo-Json
Set-Content -Path $firefoxManifest -Value $firefox -Encoding ascii
Write-Host "wrote $firefoxManifest"

# Registry root: HKLM for all users, else HKCU.
if ($AllUsers) { $root = "HKLM:\SOFTWARE" } else { $root = "HKCU:\SOFTWARE" }

function Register-Host([string]$regPath, [string]$manifestFile) {
    if (-not (Test-Path $regPath)) { New-Item -Path $regPath -Force | Out-Null }
    # The (Default) value must point at the manifest JSON.
    Set-Item -Path $regPath -Value $manifestFile
    Write-Host "registered $regPath -> $manifestFile"
}

# Chromium browsers.
Register-Host "$root\Google\Chrome\NativeMessagingHosts\$HOST_NAME" $chromeManifest
Register-Host "$root\Microsoft\Edge\NativeMessagingHosts\$HOST_NAME" $chromeManifest
# Firefox.
Register-Host "$root\Mozilla\NativeMessagingHosts\$HOST_NAME" $firefoxManifest

Write-Host "`nNative host registered for Chrome, Edge, and Firefox."
Write-Host "Load the extension (chrome://extensions -> Load unpacked -> extension\ folder) and downloads will route to the service."
