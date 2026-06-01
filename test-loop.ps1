# ALVR Test Loop Script
# Automated build-test-analyze cycle

param(
    [int]$TestDuration = 180,
    [switch]$SkipBuild,
    [switch]$Release,
    [string]$ClientPackage = "alvr.client.dev"
)

$ErrorActionPreference = "Continue"
$ProjectRoot = $PSScriptRoot
$LogDir = "$ProjectRoot\test-logs"
$BuildDir = "$ProjectRoot\build\alvr_streamer_windows"
$AdbPath = "C:\Users\jurre\AppData\Local\Android\Sdk\platform-tools\adb.exe"

# Create log directory
if (-not (Test-Path $LogDir)) {
    New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
}

function Write-Log($message, $color = "White") {
    $timestamp = Get-Date -Format "HH:mm:ss"
    Write-Host "[$timestamp] $message" -ForegroundColor $color
}

function Write-Step($message) {
    Write-Host "`n========================================" -ForegroundColor Cyan
    Write-Host "  $message" -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan
}

function Stop-AllProcesses {
    Write-Step "Stopping all VR processes"

    # Use the kill_vr_related_processes script
    $killScript = "$ProjectRoot\scripts\kill_vr_related_processes.ps1"
    if (Test-Path $killScript) {
        & $killScript
    } else {
        # Fallback if script doesn't exist
        Write-Log "Kill script not found, using fallback..." "Yellow"

        # ALVR
        Stop-Process -Name "ALVR Dashboard", "alvr_dashboard" -Force -ErrorAction SilentlyContinue

        # SteamVR
        Stop-Process -Name "vrserver", "vrmonitor", "vrcompositor", "vrdashboard" -Force -ErrorAction SilentlyContinue

        Start-Sleep -Seconds 2
    }

    Write-Log "All processes stopped" "Green"
}

function Start-Build {
    Write-Step "Building ALVR"

    # Backup session.json to session.bak first (preserve runtime changes)
    $sessionJson = "$BuildDir\session.json"
    $sessionBak = "$ProjectRoot\build\session.bak"
    if (Test-Path $sessionJson) {
        Copy-Item $sessionJson $sessionBak -Force
        Write-Log "Backed up session.json -> session.bak" "Green"
    }

    Push-Location $ProjectRoot
    try {
        $buildArgs = @("xtask", "build-streamer")
        if ($Release) { $buildArgs += "--release" }

        & cargo @buildArgs
        if ($LASTEXITCODE -ne 0) {
            Write-Log "Build failed!" "Red"
            return $false
        }

        # Copy DLLs
        $depsDir = "$ProjectRoot\deps\windows"
        $binDir = "$BuildDir\bin\win64"

        if (Test-Path "$depsDir\libvpl\alvr_build\bin") {
            Get-ChildItem "$depsDir\libvpl\alvr_build\bin\*.dll" | ForEach-Object {
                Copy-Item $_.FullName $binDir -Force
            }
        }

        # Copy session.bak to session.json (wired enabled)
        if (Test-Path $sessionBak) {
            Copy-Item $sessionBak "$BuildDir\session.json" -Force
            Write-Log "Restored session.bak -> session.json"
        }

        Write-Log "Build complete" "Green"
        return $true
    }
    finally {
        Pop-Location
    }
}

function Start-Dashboard {
    Write-Step "Starting Dashboard"

    $dashboardExe = "$BuildDir\ALVR Dashboard.exe"
    if (-not (Test-Path $dashboardExe)) {
        Write-Log "Dashboard not found at $dashboardExe" "Red"
        return $false
    }

    Start-Process -FilePath $dashboardExe -WorkingDirectory $BuildDir
    Write-Log "Dashboard started" "Green"
    Start-Sleep -Seconds 5
    return $true
}

function Start-SteamVR {
    Write-Step "Starting SteamVR"

    # Start via Steam protocol (most reliable)
    Write-Log "Starting SteamVR via Steam..."
    Start-Process "steam://run/250820"

    # Wait for SteamVR to start
    $attempts = 0
    $maxAttempts = 30
    while ($attempts -lt $maxAttempts) {
        Start-Sleep -Seconds 2
        $vrserver = Get-Process -Name "vrserver" -ErrorAction SilentlyContinue
        if ($vrserver) {
            Write-Log "SteamVR is running" "Green"
            Start-Sleep -Seconds 5  # Give it time to fully initialize
            return $true
        }
        $attempts++
        Write-Host "." -NoNewline
    }

    Write-Log "SteamVR failed to start after $maxAttempts attempts" "Red"
    return $false
}

function Start-Client {
    Write-Step "Starting Client on Quest"

    # Check device connected
    $devices = & $AdbPath devices 2>&1 | Out-String
    if ($devices -notmatch "\tdevice") {
        Write-Log "No ADB device connected. Output: $devices" "Red"
        return $false
    }

    Write-Log "Device connected, preparing device..."

    # Wake up the device
    Write-Log "Waking up device..."
    & $AdbPath shell input keyevent KEYCODE_WAKEUP 2>&1 | Out-Null

    # Disable proximity sensor (keeps screen on when headset removed)
    Write-Log "Disabling proximity sensor..."
    & $AdbPath shell "settings put system proximity_on_wake 0" 2>&1 | Out-Null
    & $AdbPath shell "settings put secure proximity_on_wake 0" 2>&1 | Out-Null
    # Alternative method for Quest - use developer bypass
    & $AdbPath shell "setprop debug.oculus.proximityBypass 1" 2>&1 | Out-Null

    # Keep screen on while charging/USB connected
    & $AdbPath shell "settings put global stay_on_while_plugged_in 3" 2>&1 | Out-Null

    Write-Log "Device prepared, launching $ClientPackage..."

    # Method 1: Use monkey (works better for VR apps)
    $result = & $AdbPath shell monkey -p $ClientPackage -c android.intent.category.LAUNCHER 1 2>&1

    if ($result -match "Events injected: 1") {
        Write-Log "Client launched via monkey" "Green"
        Start-Sleep -Seconds 5
        return $true
    }

    # Method 2: Fallback to am start
    Write-Log "Trying am start fallback..."
    & $AdbPath shell am start -n "$ClientPackage/android.app.NativeActivity" 2>&1

    # Method 3: Alternative with action
    & $AdbPath shell am start -a android.intent.action.MAIN -c android.intent.category.LAUNCHER -n "$ClientPackage/android.app.NativeActivity" 2>&1

    Start-Sleep -Seconds 5

    # Verify client is running
    $running = & $AdbPath shell pidof $ClientPackage 2>&1
    if ($running -match "\d+") {
        Write-Log "Client is running (PID: $running)" "Green"
        return $true
    } else {
        Write-Log "Client may not have started - please check headset" "Yellow"
        return $true  # Continue anyway, user might need to manually start
    }
}

function Get-Logs {
    param([string]$iteration)

    Write-Step "Retrieving Logs"

    $timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $iterationLogDir = "$LogDir\iteration_${iteration}_$timestamp"
    New-Item -ItemType Directory -Path $iterationLogDir -Force | Out-Null

    # Get stored logs from ALVR dashboard API (new endpoint!)
    Write-Log "Fetching stored logs from dashboard API..."
    try {
        $response = Invoke-WebRequest -Uri "http://localhost:8082/api/diagnostics/full" -TimeoutSec 10 -ErrorAction SilentlyContinue
        if ($response.StatusCode -eq 200) {
            $response.Content | Out-File "$iterationLogDir\alvr_diagnostics.json" -Encoding UTF8
            Write-Log "Saved ALVR diagnostics (API)" "Green"

            # Also save just the logs in readable format
            $data = $response.Content | ConvertFrom-Json
            if ($data.logs) {
                $data.logs | ForEach-Object {
                    "[$($_.timestamp)] [$($_.source)] [$($_.severity)] $($_.content)"
                } | Out-File "$iterationLogDir\alvr_logs.txt" -Encoding UTF8
                Write-Log "Saved $($data.logs.Count) log entries"
            }
        }
    }
    catch {
        Write-Log "Could not fetch from dashboard API: $_" "Yellow"
    }

    # Get streamer session.json
    if (Test-Path "$BuildDir\session.json") {
        Copy-Item "$BuildDir\session.json" "$iterationLogDir\session.json"
        Write-Log "Copied session.json"
    }

    # Get crash log if exists
    if (Test-Path "$BuildDir\crash_log.txt") {
        Copy-Item "$BuildDir\crash_log.txt" "$iterationLogDir\crash_log.txt"
        Write-Log "Copied crash_log.txt"
    }

    # Get client logs via ADB
    Write-Log "Fetching client logcat..."
    & $AdbPath logcat -d -s "alvr_client","Unity","OpenXR" > "$iterationLogDir\client_logcat.txt" 2>&1

    # Get ALVR-specific logs
    & $AdbPath logcat -d | Select-String -Pattern "ALVR|alvr" > "$iterationLogDir\alvr_logcat.txt" 2>&1

    # Get SteamVR logs
    $steamvrLog = "C:\Program Files (x86)\Steam\logs\vrserver.txt"
    if (Test-Path $steamvrLog) {
        Copy-Item $steamvrLog "$iterationLogDir\vrserver.txt"
        Write-Log "Copied vrserver.txt"
    }

    Write-Log "Logs saved to: $iterationLogDir" "Green"
    return $iterationLogDir
}

function Analyze-Logs {
    param([string]$logDir)

    Write-Step "Analyzing Logs"

    $analysis = @{
        Errors = @()
        Warnings = @()
        ConnectionStatus = "Unknown"
        FrameStats = @{}
    }

    # Analyze crash log
    $crashLog = "$logDir\crash_log.txt"
    if (Test-Path $crashLog) {
        $crashContent = Get-Content $crashLog -Raw
        if ($crashContent) {
            Write-Log "CRASH DETECTED!" "Red"
            $analysis.Errors += "Crash occurred - see crash_log.txt"
        }
    }

    # Analyze client logcat for errors
    $clientLog = "$logDir\client_logcat.txt"
    if (Test-Path $clientLog) {
        $errors = Select-String -Path $clientLog -Pattern "ERROR|FATAL|Exception|panic" -ErrorAction SilentlyContinue
        if ($errors) {
            $analysis.Errors += $errors | ForEach-Object { $_.Line }
        }
    }

    # Analyze ALVR logs for connection issues
    $alvrLog = "$logDir\alvr_logcat.txt"
    if (Test-Path $alvrLog) {
        $content = Get-Content $alvrLog -Raw

        if ($content -match "Connected") {
            $analysis.ConnectionStatus = "Connected"
            Write-Log "Connection: SUCCESS" "Green"
        }
        elseif ($content -match "Handshake|Try again") {
            $analysis.ConnectionStatus = "Handshake Failed"
            Write-Log "Connection: HANDSHAKE FAILED" "Red"
        }
        else {
            $analysis.ConnectionStatus = "No Connection"
            Write-Log "Connection: NONE" "Yellow"
        }
    }

    # Print summary
    Write-Host "`n--- Analysis Summary ---" -ForegroundColor Magenta
    Write-Host "Connection: $($analysis.ConnectionStatus)"
    Write-Host "Errors: $($analysis.Errors.Count)"

    if ($analysis.Errors.Count -gt 0) {
        Write-Host "`nErrors found:" -ForegroundColor Red
        $analysis.Errors | Select-Object -First 10 | ForEach-Object {
            Write-Host "  - $_" -ForegroundColor Red
        }
    }

    # Save analysis
    $analysis | ConvertTo-Json -Depth 3 | Out-File "$logDir\analysis.json"

    return $analysis
}

function Wait-TestDuration {
    Write-Step "Running test for $TestDuration seconds"

    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    while ($stopwatch.Elapsed.TotalSeconds -lt $TestDuration) {
        $remaining = $TestDuration - [int]$stopwatch.Elapsed.TotalSeconds
        Write-Progress -Activity "Test in progress" -Status "$remaining seconds remaining" -PercentComplete (($stopwatch.Elapsed.TotalSeconds / $TestDuration) * 100)
        Start-Sleep -Seconds 5

        # Check if processes are still running
        $dashboard = Get-Process -Name "ALVR Dashboard" -ErrorAction SilentlyContinue
        $vrserver = Get-Process -Name "vrserver" -ErrorAction SilentlyContinue

        if (-not $dashboard) {
            Write-Log "Dashboard crashed!" "Red"
            break
        }
        if (-not $vrserver) {
            Write-Log "SteamVR crashed!" "Red"
            break
        }
    }
    Write-Progress -Activity "Test in progress" -Completed
}

# ============================================
# MAIN
# ============================================

Write-Host @"

    _    _ __     ______  _____         _
   / \  | |\ \   / /  _ \|_   _|__  ___| |_
  / _ \ | | \ \ / /| |_) | | |/ _ \/ __| __|
 / ___ \| |__\ V / |  _ <  | |  __/\__ \ |_
/_/   \_\_____\_/  |_| \_\ |_|\___||___/\__|

"@ -ForegroundColor Cyan

Write-Host "Configuration:" -ForegroundColor Yellow
Write-Host "  Test Duration: $TestDuration seconds"
Write-Host "  Skip Build: $SkipBuild"
Write-Host "  Client Package: $ClientPackage"
Write-Host "  Log Directory: $LogDir"
Write-Host ""

# 1. Stop all processes
Stop-AllProcesses

# 2. Build (unless skipped)
if (-not $SkipBuild) {
    $buildSuccess = Start-Build
    if (-not $buildSuccess) {
        Write-Log "Build failed!" "Red"
        exit 1
    }
}

# 3. Start Dashboard
$dashboardStarted = Start-Dashboard
if (-not $dashboardStarted) {
    Write-Log "Dashboard failed to start!" "Red"
    exit 1
}

# 4. Start SteamVR
$steamvrStarted = Start-SteamVR
if (-not $steamvrStarted) {
    Write-Log "SteamVR failed to start, continuing anyway..." "Yellow"
}

# 5. Start Client
$clientStarted = Start-Client
if (-not $clientStarted) {
    Write-Log "Client failed to start, continuing anyway..." "Yellow"
}

# 6. Wait for test duration
Wait-TestDuration

# 7. Retrieve logs
$logDir = Get-Logs -iteration "1"

# 8. Analyze logs
$analysis = Analyze-Logs -logDir $logDir

Write-Host "`n=== Test Complete ===" -ForegroundColor Green
Write-Host "Logs saved to: $logDir"
