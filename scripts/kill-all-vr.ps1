# Kill All VR Processes
# Stops ALVR, SteamVR, and related processes

$ErrorActionPreference = "SilentlyContinue"

Write-Host "Killing VR processes..." -ForegroundColor Yellow

# ALVR
$alvr = @("ALVR Dashboard", "alvr_dashboard", "ALVR Launcher", "alvr_launcher")
foreach ($p in $alvr) {
    $proc = Get-Process -Name $p -ErrorAction SilentlyContinue
    if ($proc) {
        Write-Host "  Killing $p..." -ForegroundColor Cyan
        Stop-Process -Name $p -Force
    }
}

# SteamVR - comprehensive list
$steamvr = @(
    "vrserver",
    "vrmonitor",
    "vrcompositor",
    "vrdashboard",
    "vrwebhelper",
    "vrstartup",
    "vrservice",
    "steamvr_tutorial",
    "steamtours",
    "vrpathregistryui",
    "steamvr_desktop_game_theater",
    "steamvr_room_setup"
)
foreach ($p in $steamvr) {
    $proc = Get-Process -Name $p -ErrorAction SilentlyContinue
    if ($proc) {
        Write-Host "  Killing $p..." -ForegroundColor Cyan
        Stop-Process -Name $p -Force
    }
}

# Virtual Desktop
$vd = @("Virtual Desktop Streamer", "VirtualDesktop.Streamer", "Virtual Desktop")
foreach ($p in $vd) {
    $proc = Get-Process -Name $p -ErrorAction SilentlyContinue
    if ($proc) {
        Write-Host "  Killing $p..." -ForegroundColor Cyan
        Stop-Process -Name $p -Force
    }
}

# Oculus/Meta
$oculus = @(
    "OculusClient",
    "OVRServer_x64",
    "OVRServiceLauncher",
    "OVRRedir",
    "oculus-platform-runtime"
)
foreach ($p in $oculus) {
    $proc = Get-Process -Name $p -ErrorAction SilentlyContinue
    if ($proc) {
        Write-Host "  Killing $p..." -ForegroundColor Cyan
        Stop-Process -Name $p -Force
    }
}

Start-Sleep -Seconds 2

# Verify
Write-Host "`nChecking remaining VR processes..." -ForegroundColor Yellow
$allProcs = $alvr + $steamvr + $vd + $oculus
$remaining = Get-Process -Name $allProcs -ErrorAction SilentlyContinue
if ($remaining) {
    Write-Host "Some processes still running:" -ForegroundColor Red
    $remaining | ForEach-Object { Write-Host "  $($_.Name) (PID: $($_.Id))" -ForegroundColor Red }
} else {
    Write-Host "All VR processes stopped." -ForegroundColor Green
}
