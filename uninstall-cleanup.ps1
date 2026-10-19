# Stops + deletes the DownloadManager service and browser host registration.
$svc = "DownloadManager"
$existing = Get-Service -Name $svc -ErrorAction SilentlyContinue
if ($existing) {
    if ($existing.Status -eq 'Running') { Stop-Service -Name $svc -Force -ErrorAction SilentlyContinue }
    sc.exe delete $svc | Out-Null
}
$hn = "com.downloadmanager.host"
foreach ($r in @(
    "HKLM:\SOFTWARE\Google\Chrome\NativeMessagingHosts\$hn",
    "HKLM:\SOFTWARE\Microsoft\Edge\NativeMessagingHosts\$hn",
    "HKLM:\SOFTWARE\Mozilla\NativeMessagingHosts\$hn")) {
    Remove-Item -Path $r -Recurse -Force -ErrorAction SilentlyContinue
}
