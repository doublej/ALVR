# kill_and_start_all_vr.ps1
# Restart ALVR after killing every VR-related and ADB process, then re-establish USB and Wi-Fi ADB transports.

Write-Host "Terminating adb, ALVR, Steam, and SteamVR processes..." -ForegroundColor Yellow

# Processes to kill (add any stragglers you have seen)
$processesToKill = @(
    "adb",               # Android Debug Bridge
    "ALVR",              # ALVR main process
    "ALVRDashboard",     # ALVR Dashboard
    "alvr_dashboard",
    "ALVR Dashboard",
    "alvr_launcher",
    "alvr launcher",
    "Steam",             # Steam client
    "Steamvr",             # Steam client
    "vrserver",          # SteamVR server
    "vrcompositor",      # SteamVR compositor
    "vrmonitor",         # SteamVR monitor
    "OculusClient"
)

# Hard-kill every process name in the list
foreach ($p in $processesToKill) {
    Get-Process -Name $p -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Get-Process -Name "$p.exe" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

Write-Host "Waiting 3 seconds for processes to terminate..." -ForegroundColor Yellow
Start-Sleep -Seconds 3

# Extra brute-force for adb and ALVR Dashboard in case they respawn
Start-Process taskkill.exe -ArgumentList "/F","/T","/IM","adb.exe" -NoNewWindow -Wait
Start-Process taskkill.exe -ArgumentList "/F","/T","/IM","adb" -NoNewWindow -Wait
Start-Process taskkill.exe -ArgumentList "/F","/T","/IM","ALVR Dashboard.exe" -NoNewWindow -Wait
