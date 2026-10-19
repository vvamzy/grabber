# Registers the DownloadManager Windows service. Requires Administrator.
# Used by dev testing and mirrored by the Inno Setup installer's [Run] section.
$ErrorActionPreference = "Stop"

$exe = Join-Path $PSScriptRoot "target\release\dm-service.exe"
if (-not (Test-Path $exe)) { throw "dm-service.exe not found at $exe" }

$svc = "DownloadManager"

# Stop + remove any existing instance so re-install is clean.
$existing = Get-Service -Name $svc -ErrorAction SilentlyContinue
if ($existing) {
    Write-Host "Removing existing '$svc' service..."
    if ($existing.Status -eq 'Running') { Stop-Service -Name $svc -Force -ErrorAction SilentlyContinue }
    sc.exe delete $svc | Out-Null
    Start-Sleep -Seconds 1
}

# Create (auto-start at boot) + start.
Write-Host "Creating '$svc' service..."
New-Service -Name $svc `
    -BinaryPathName "`"$exe`"" `
    -DisplayName "Download Manager" `
    -Description "Multi-segment download manager. Loopback API on 127.0.0.1:7790." `
    -StartupType Automatic | Out-Null

Write-Host "Starting '$svc'..."
Start-Service -Name $svc
Start-Sleep -Seconds 1

Get-Service -Name $svc | Format-Table Status, Name, DisplayName -AutoSize

# Quick health check against the loopback API.
try {
    $ping = Invoke-WebRequest -Uri "http://127.0.0.1:7790/api/ping" -UseBasicParsing -TimeoutSec 5
    Write-Host "API health: $($ping.Content)"
} catch {
    Write-Warning "Service started but API not reachable yet: $_"
}
