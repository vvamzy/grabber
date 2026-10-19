# Creates + starts the DownloadManager service. Run elevated (installer does).
param([string]$Dir = $PSScriptRoot)
$ErrorActionPreference = "Stop"
$exe = Join-Path $Dir "dm-service.exe"
if (-not (Test-Path $exe)) { throw "dm-service.exe not found at $exe" }
$svc = "DownloadManager"
$existing = Get-Service -Name $svc -ErrorAction SilentlyContinue
if ($existing) {
    if ($existing.Status -eq 'Running') { Stop-Service -Name $svc -Force -ErrorAction SilentlyContinue }
    sc.exe delete $svc | Out-Null
    Start-Sleep -Seconds 1
}
New-Service -Name $svc `
    -BinaryPathName "`"$exe`"" `
    -DisplayName "Download Manager" `
    -Description "Multi-segment download manager. Loopback API on 127.0.0.1:7790." `
    -StartupType Automatic | Out-Null
Start-Service -Name $svc
Start-Sleep -Seconds 1
try {
    $ping = Invoke-WebRequest -Uri "http://127.0.0.1:7790/api/ping" -UseBasicParsing -TimeoutSec 5
    Write-Host "API health: $($ping.Content)"
} catch {
    Write-Warning "Service started but API not reachable yet: $_"
}
