# Stops and removes the DownloadManager Windows service. Requires Administrator.
$ErrorActionPreference = "Stop"
$svc = "DownloadManager"

$existing = Get-Service -Name $svc -ErrorAction SilentlyContinue
if (-not $existing) {
    Write-Host "'$svc' is not installed."
    return
}

Write-Host "Stopping '$svc'..."
if ($existing.Status -eq 'Running') { Stop-Service -Name $svc -Force -ErrorAction SilentlyContinue }
Start-Sleep -Seconds 1

Write-Host "Deleting '$svc'..."
sc.exe delete $svc | Out-Null
Write-Host "Done. (Files in %PROGRAMDATA%\download-manager are left in place.)"
