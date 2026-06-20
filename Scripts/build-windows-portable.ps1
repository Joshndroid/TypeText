$ErrorActionPreference = "Stop"

$RootDir = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "version.ps1")
$Version = Get-TypeTextVersion -RootDir $RootDir
$env:TYPETEXT_VERSION = $Version
$WindowsTarget = $env:TYPETEXT_WINDOWS_TARGET
if ([string]::IsNullOrWhiteSpace($WindowsTarget)) {
    $WindowsTarget = "x86_64-pc-windows-msvc"
}

$DistDir = Join-Path $RootDir "dist\TypeText-Windows"
$ZipPath = Join-Path $RootDir "dist\TypeText-Windows-x64.zip"
$DataDir = Join-Path $DistDir "data"
$ReleaseDir = Join-Path $RootDir "target\$WindowsTarget\release"
$ExeSource = Join-Path $ReleaseDir "typetext-desktop.exe"
$ExeDest = Join-Path $DistDir "TypeText.exe"
$OfflineDistDir = Join-Path $RootDir "dist\TypeText-Windows-Offline-Portable"
$OfflineZipPath = Join-Path $RootDir "dist\TypeText-Windows-x64-Offline-Portable.zip"
$OfflineDataDir = Join-Path $OfflineDistDir "data"
$OfflineExeDest = Join-Path $OfflineDistDir "TypeText.exe"

Set-Location $RootDir
Write-Host "Building TypeText for Windows target: $WindowsTarget"
Write-Host "Version: $Version"
Write-Host "If the target is missing, run: rustup target add $WindowsTarget"
cargo build --release --target $WindowsTarget -p typetext-desktop

if (Test-Path $DistDir) {
    Remove-Item $DistDir -Recurse -Force
}

New-Item -ItemType Directory -Path $DataDir -Force | Out-Null
Copy-Item $ExeSource $ExeDest
Invoke-TypeTextOptionalSigning -Path $ExeDest

$SnippetsSource = Join-Path $RootDir "examples\snippets.json"
if (Test-Path $SnippetsSource) {
    Copy-Item $SnippetsSource (Join-Path $DataDir "snippets.json")
}

$SettingsSource = Join-Path $RootDir "examples\settings.json"
if (Test-Path $SettingsSource) {
    Copy-Item $SettingsSource (Join-Path $DataDir "settings.json")
}

if (Test-Path $ZipPath) {
    Remove-Item $ZipPath -Force
}
Compress-Archive -Path $DistDir -DestinationPath $ZipPath -Force
Write-TypeTextSha256Checksum -Path $ZipPath

Write-Host "Building offline portable TypeText for Windows target: $WindowsTarget"
cargo build --release --target $WindowsTarget -p typetext-desktop --features offline-portable

if (Test-Path $OfflineDistDir) {
    Remove-Item $OfflineDistDir -Recurse -Force
}
New-Item -ItemType Directory -Path $OfflineDataDir -Force | Out-Null
Copy-Item $ExeSource $OfflineExeDest
Invoke-TypeTextOptionalSigning -Path $OfflineExeDest

if (Test-Path $SnippetsSource) {
    Copy-Item $SnippetsSource (Join-Path $OfflineDataDir "snippets.json")
}
if (Test-Path $SettingsSource) {
    Copy-Item $SettingsSource (Join-Path $OfflineDataDir "settings.json")
}
if (Test-Path $OfflineZipPath) {
    Remove-Item $OfflineZipPath -Force
}
Compress-Archive -Path $OfflineDistDir -DestinationPath $OfflineZipPath -Force
Write-TypeTextSha256Checksum -Path $OfflineZipPath

Write-Host "Built $DistDir"
Write-Host "Archived $ZipPath"
Write-Host "Built $OfflineDistDir"
Write-Host "Archived $OfflineZipPath"
Write-Host "Run with: $ExeDest"
