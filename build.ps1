# ALVR Build Script
# Usage: .\build.ps1 [-Streamer] [-Client] [-Launcher] [-All] [-Clean] [-Release] [-Gpl] [-Deploy]

param(
    [switch]$Streamer,
    [switch]$Client,
    [switch]$Launcher,
    [switch]$All,
    [switch]$Clean,
    [switch]$Release,
    [switch]$Gpl,
    [switch]$Deploy  # Deploy client APK to connected device via ADB
)

$ErrorActionPreference = "Stop"
$ProjectRoot = $PSScriptRoot

# Paths
$BuildDir = "$ProjectRoot\build"

# Generate timestamped log file path
$LogTimestamp = Get-Date -Format "yyyy-MM-dd_HH-mm-ss"
$LogFile = "$BuildDir\build_$LogTimestamp.log"

# Start transcript to capture all console output
if (-not (Test-Path $BuildDir)) {
    New-Item -ItemType Directory -Path $BuildDir -Force | Out-Null
}
Start-Transcript -Path $LogFile -Append
$StreamerBuildDir = "$BuildDir\alvr_streamer_windows"
$LauncherBuildDir = "$BuildDir\alvr_launcher_windows"
$ClientApk = "$ProjectRoot\target\release\apk\alvr_client_openxr.apk"
$DepsDir = "$ProjectRoot\deps\windows"
$DriverBinDir = "$StreamerBuildDir\bin\win64"

# Android SDK paths
$env:ANDROID_HOME = "C:\Users\jurre\AppData\Local\Android\Sdk"
$env:ANDROID_NDK_HOME = "C:\Users\jurre\AppData\Local\Android\Sdk\ndk\26.1.10909125"
$env:JAVA_HOME = "C:\Program Files\Android\Android Studio\jbr"

# Default to streamer if nothing specified
if (-not ($Streamer -or $Client -or $Launcher -or $All)) {
    $Streamer = $true
}

if ($All) {
    $Streamer = $true
    $Client = $true
    $Launcher = $true
}

function Write-Step($message) {
    Write-Host "`n=== $message ===" -ForegroundColor Cyan
}

function Backup-SessionConfig {
    Write-Step "Backing up session config"

    $sessionJson = "$StreamerBuildDir\session.json"
    $sessionBak = "$BuildDir\session.bak"

    if (Test-Path $sessionJson) {
        Copy-Item $sessionJson $sessionBak -Force
        Write-Host "  Copied session.json -> session.bak" -ForegroundColor Green
    } else {
        Write-Host "  [INFO] No session.json found to backup" -ForegroundColor Yellow
    }
}

function Clean-Build {
    Write-Step "Cleaning old build artifacts"

    if ($Streamer -or $All) {
        if (Test-Path "$StreamerBuildDir\ALVR Dashboard.exe") {
            Write-Host "  Removing old Dashboard exe..."
            Remove-Item "$StreamerBuildDir\ALVR Dashboard.exe" -Force
        }
        if (Test-Path "$DriverBinDir\driver_alvr_server.dll") {
            Write-Host "  Removing old driver DLL..."
            Remove-Item "$DriverBinDir\driver_alvr_server.dll" -Force
        }
    }

    if ($Launcher -or $All) {
        if (Test-Path "$LauncherBuildDir\ALVR Launcher.exe") {
            Write-Host "  Removing old Launcher exe..."
            Remove-Item "$LauncherBuildDir\ALVR Launcher.exe" -Force
        }
    }

    if ($Client -or $All) {
        if (Test-Path $ClientApk) {
            Write-Host "  Removing old client APK..."
            Remove-Item $ClientApk -Force
        }
    }
}

function Copy-DependencyDlls {
    Write-Step "Copying dependency DLLs"

    # Ensure bin directory exists
    if (-not (Test-Path $DriverBinDir)) {
        New-Item -ItemType Directory -Path $DriverBinDir -Force | Out-Null
    }

    # Copy libvpl and MSVC runtime DLLs
    $libvplDir = "$DepsDir\libvpl\alvr_build\bin"
    if (Test-Path $libvplDir) {
        Write-Host "  Copying libvpl DLLs..."
        Get-ChildItem "$libvplDir\*.dll" | ForEach-Object {
            Copy-Item $_.FullName $DriverBinDir -Force
            Write-Host "    $($_.Name)" -ForegroundColor Gray
        }
    }

    # Copy FFmpeg DLLs if --gpl flag used
    if ($Gpl) {
        $ffmpegDir = "$DepsDir\ffmpeg\bin"
        if (Test-Path $ffmpegDir) {
            Write-Host "  Copying FFmpeg DLLs..."
            Get-ChildItem "$ffmpegDir\*.dll" | ForEach-Object {
                Copy-Item $_.FullName $DriverBinDir -Force
                Write-Host "    $($_.Name)" -ForegroundColor Gray
            }
        }

        # Copy x264 DLL
        $x264Dll = "$DepsDir\x264\bin\x64\x264.dll"
        if (Test-Path $x264Dll) {
            Write-Host "  Copying x264.dll..."
            Copy-Item $x264Dll $DriverBinDir -Force
        }
    }
}

function Verify-Dlls {
    Write-Step "Verifying DLLs"

    $dlls = Get-ChildItem "$DriverBinDir\*.dll" -ErrorAction SilentlyContinue
    if ($dlls) {
        foreach ($dll in $dlls) {
            $size = $dll.Length / 1MB
            Write-Host ("  [OK] {0} ({1:N2} MB)" -f $dll.Name, $size) -ForegroundColor Green
        }
        Write-Host "`n  Total: $($dlls.Count) DLLs" -ForegroundColor Yellow
    } else {
        Write-Host "  [WARNING] No DLLs found in $DriverBinDir" -ForegroundColor Red
    }
}

function Copy-SessionConfig {
    Write-Step "Copying session config (wired enabled)"

    $sessionBak = "$BuildDir\session.bak"
    $sessionJson = "$StreamerBuildDir\session.json"

    if (Test-Path $sessionBak) {
        Copy-Item $sessionBak $sessionJson -Force
        Write-Host "  Copied session.bak -> session.json" -ForegroundColor Green
    } else {
        Write-Host "  [WARNING] session.bak not found at $sessionBak" -ForegroundColor Yellow
    }
}

function Build-Streamer {
    Write-Step "Building Streamer"
    Push-Location $ProjectRoot
    try {
        $buildArgs = @("xtask", "build-streamer")
        if ($Release) { $buildArgs += "--release" }
        if ($Gpl) { $buildArgs += "--gpl" }

        & cargo @buildArgs
        if ($LASTEXITCODE -ne 0) { throw "Streamer build failed" }

        Write-Host "`nStreamer build complete:" -ForegroundColor Green
        Write-Host "  Dashboard: $StreamerBuildDir\ALVR Dashboard.exe"
        Write-Host "  Driver:    $DriverBinDir\driver_alvr_server.dll"
    }
    finally {
        Pop-Location
    }
}

function Build-Client {
    Write-Step "Building Client"
    Push-Location $ProjectRoot
    try {
        $buildArgs = @("xtask", "build-client")
        if ($Release) { $buildArgs += "--release" }

        & cargo @buildArgs
        if ($LASTEXITCODE -ne 0) { throw "Client build failed" }

        Write-Host "`nClient build complete:" -ForegroundColor Green
        Write-Host "  APK: $ClientApk"
    }
    finally {
        Pop-Location
    }
}

function Deploy-Client {
    Write-Step "Deploying Client APK"

    if (-not (Test-Path $ClientApk)) {
        Write-Host "  [ERROR] APK not found at $ClientApk" -ForegroundColor Red
        return
    }

    # Find ADB
    $AdbPath = "$env:ANDROID_HOME\platform-tools\adb.exe"
    if (-not (Test-Path $AdbPath)) {
        $AdbPath = (Get-Command adb -ErrorAction SilentlyContinue).Source
    }
    if (-not $AdbPath) {
        Write-Host "  [ERROR] ADB not found. Install Android SDK platform-tools." -ForegroundColor Red
        return
    }

    Write-Host "  Using ADB: $AdbPath"

    # Check for connected devices
    $devices = & $AdbPath devices | Select-String -Pattern "^\S+\s+device$"
    if (-not $devices) {
        Write-Host "  [ERROR] No authorized device connected." -ForegroundColor Red
        Write-Host "  Make sure:" -ForegroundColor Yellow
        Write-Host "    1. Device is connected via USB" -ForegroundColor Yellow
        Write-Host "    2. USB debugging is enabled" -ForegroundColor Yellow
        Write-Host "    3. Device is authorized (check headset for prompt)" -ForegroundColor Yellow
        return
    }

    $deviceSerial = ($devices -split '\s+')[0]
    Write-Host "  Found device: $deviceSerial" -ForegroundColor Green

    # Uninstall existing (ignore errors if not installed)
    Write-Host "  Uninstalling existing ALVR client..."
    & $AdbPath -s $deviceSerial uninstall alvr.client.dev 2>$null
    & $AdbPath -s $deviceSerial uninstall alvr.client.stable 2>$null
    & $AdbPath -s $deviceSerial uninstall alvr.client 2>$null

    # Install new APK
    Write-Host "  Installing $ClientApk..."
    & $AdbPath -s $deviceSerial install -r $ClientApk
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  [ERROR] APK installation failed" -ForegroundColor Red
        return
    }

    Write-Host "  [OK] Client APK deployed successfully!" -ForegroundColor Green

    # Optionally launch the app
    Write-Host "  Launching ALVR client..."
    & $AdbPath -s $deviceSerial shell am start -n alvr.client.dev/android.app.NativeActivity 2>$null
    if ($LASTEXITCODE -eq 0) {
        Write-Host "  [OK] ALVR client launched" -ForegroundColor Green
    }
}

function Build-Launcher {
    Write-Step "Building Launcher"
    Push-Location $ProjectRoot
    try {
        $buildArgs = @("xtask", "build-launcher")
        if ($Release) { $buildArgs += "--release" }

        & cargo @buildArgs
        if ($LASTEXITCODE -ne 0) { throw "Launcher build failed" }

        Write-Host "`nLauncher build complete:" -ForegroundColor Green
        Write-Host "  Launcher: $LauncherBuildDir\ALVR Launcher.exe"
    }
    finally {
        Pop-Location
    }
}

# Main execution
Write-Host "ALVR Build Script" -ForegroundColor Yellow
Write-Host "Project: $ProjectRoot"
Write-Host "Build targets: $(if($Streamer){'Streamer '})$(if($Client){'Client '})$(if($Launcher){'Launcher'})"
Write-Host "Options: $(if($Release){'Release '})$(if($Gpl){'GPL '})$(if($Deploy){'Deploy '})"

if ($Clean) {
    Clean-Build
}

if ($Streamer) {
    Backup-SessionConfig
    Clean-Build
    Build-Streamer
    Copy-DependencyDlls
    Verify-Dlls
    Copy-SessionConfig
}

if ($Client) {
    Build-Client

    if ($Deploy) {
        Deploy-Client
    }
}

if ($Launcher) {
    Build-Launcher
}

Write-Host "`n=== Build Complete ===" -ForegroundColor Green

# Stop transcript and show log location
Stop-Transcript
Write-Host "Build log saved to: $LogFile" -ForegroundColor Cyan
