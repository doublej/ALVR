# ALVR Post-Build DLL Copy Script
# Copies required DLLs to the streamer build output directory

param(
    [string]$BuildDir = "$PSScriptRoot\..\build\alvr_streamer_windows",
    [switch]$Gpl  # Include FFmpeg DLLs
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path "$PSScriptRoot\..").Path
$DepsDir = "$RepoRoot\deps\windows"
$TargetDir = "$BuildDir\bin\win64"

Write-Host "ALVR DLL Copy Script" -ForegroundColor Cyan
Write-Host "===================" -ForegroundColor Cyan
Write-Host "Target directory: $TargetDir"

# Ensure target directory exists
if (-not (Test-Path $TargetDir)) {
    Write-Host "Error: Target directory does not exist. Run build-streamer first." -ForegroundColor Red
    exit 1
}

# DLL sources and their target names
$DllSources = @(
    # libvpl and its dependencies
    @{ Source = "$DepsDir\libvpl\alvr_build\bin\libvpl.dll"; Required = $true },
    @{ Source = "$DepsDir\libvpl\alvr_build\bin\concrt140.dll"; Required = $false },
    @{ Source = "$DepsDir\libvpl\alvr_build\bin\msvcp140.dll"; Required = $false },
    @{ Source = "$DepsDir\libvpl\alvr_build\bin\msvcp140_1.dll"; Required = $false },
    @{ Source = "$DepsDir\libvpl\alvr_build\bin\msvcp140_2.dll"; Required = $false },
    @{ Source = "$DepsDir\libvpl\alvr_build\bin\msvcp140_atomic_wait.dll"; Required = $false },
    @{ Source = "$DepsDir\libvpl\alvr_build\bin\msvcp140_codecvt_ids.dll"; Required = $false },
    @{ Source = "$DepsDir\libvpl\alvr_build\bin\vcruntime140.dll"; Required = $false },
    @{ Source = "$DepsDir\libvpl\alvr_build\bin\vcruntime140_1.dll"; Required = $false }
)

# Add FFmpeg DLLs if --gpl flag is set
if ($Gpl) {
    Write-Host "Including FFmpeg DLLs (GPL build)" -ForegroundColor Yellow
    $FfmpegDlls = @(
        @{ Source = "$DepsDir\ffmpeg\bin\avcodec-61.dll"; Required = $true },
        @{ Source = "$DepsDir\ffmpeg\bin\avdevice-61.dll"; Required = $false },
        @{ Source = "$DepsDir\ffmpeg\bin\avfilter-10.dll"; Required = $false },
        @{ Source = "$DepsDir\ffmpeg\bin\avformat-61.dll"; Required = $false },
        @{ Source = "$DepsDir\ffmpeg\bin\avutil-59.dll"; Required = $true },
        @{ Source = "$DepsDir\ffmpeg\bin\postproc-58.dll"; Required = $false },
        @{ Source = "$DepsDir\ffmpeg\bin\swresample-5.dll"; Required = $false },
        @{ Source = "$DepsDir\ffmpeg\bin\swscale-8.dll"; Required = $false },
        @{ Source = "$DepsDir\x264\bin\x64\x264.dll"; Required = $false }
    )
    $DllSources += $FfmpegDlls
}

$Copied = 0
$Skipped = 0
$Missing = 0

foreach ($Dll in $DllSources) {
    $SourcePath = $Dll.Source
    $FileName = Split-Path $SourcePath -Leaf
    $TargetPath = "$TargetDir\$FileName"

    if (-not (Test-Path $SourcePath)) {
        if ($Dll.Required) {
            Write-Host "  MISSING (required): $FileName" -ForegroundColor Red
            $Missing++
        } else {
            Write-Host "  MISSING (optional): $FileName" -ForegroundColor DarkGray
        }
        continue
    }

    # Check if file already exists and is identical
    if (Test-Path $TargetPath) {
        $SourceHash = (Get-FileHash $SourcePath -Algorithm MD5).Hash
        $TargetHash = (Get-FileHash $TargetPath -Algorithm MD5).Hash
        if ($SourceHash -eq $TargetHash) {
            Write-Host "  SKIP (identical): $FileName" -ForegroundColor DarkGray
            $Skipped++
            continue
        }
    }

    Copy-Item $SourcePath $TargetPath -Force
    Write-Host "  COPIED: $FileName" -ForegroundColor Green
    $Copied++
}

Write-Host ""
Write-Host "Summary: $Copied copied, $Skipped skipped, $Missing missing" -ForegroundColor Cyan

if ($Missing -gt 0) {
    Write-Host "Warning: Some required DLLs are missing. Run 'cargo xtask prepare-deps --platform windows' first." -ForegroundColor Yellow
    exit 1
}

Write-Host "Done!" -ForegroundColor Green
