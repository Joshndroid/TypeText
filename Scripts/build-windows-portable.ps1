param(
    [ValidateSet("All", "Standard", "Offline")]
    [string]$Variant = "All"
)

$ErrorActionPreference = "Stop"

$RootDir = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "version.ps1")
$Version = Get-TypeTextVersion -RootDir $RootDir
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

$SnippetsSource = Join-Path $RootDir "examples\snippets.json"
$SettingsSource = Join-Path $RootDir "examples\settings.json"
$OfflineSettingsSource = Join-Path $RootDir "examples\settings.offline.json"

Set-Location $RootDir
Write-Host "Building TypeText for Windows target: $WindowsTarget"
Write-Host "Version: $Version"
Write-Host "Variant: $Variant"
Write-Host "If the target is missing, run: rustup target add $WindowsTarget"

if ($Variant -in @("All", "Standard")) {
    cargo build --release --target $WindowsTarget -p typetext-desktop --locked

    if (Test-Path $DistDir) {
        Remove-Item $DistDir -Recurse -Force
    }
    New-Item -ItemType Directory -Path $DataDir -Force | Out-Null
    Copy-Item $ExeSource $ExeDest
    Invoke-TypeTextOptionalSigning -Path $ExeDest

    if (Test-Path $SnippetsSource) {
        Copy-Item $SnippetsSource (Join-Path $DataDir "snippets.json")
    }
    if (Test-Path $SettingsSource) {
        Copy-Item $SettingsSource (Join-Path $DataDir "settings.json")
    }
    if (Test-Path $ZipPath) {
        Remove-Item $ZipPath -Force
    }
    Compress-Archive -Path $DistDir -DestinationPath $ZipPath -Force
    Write-TypeTextSha256Checksum -Path $ZipPath

    Write-Host "Built $DistDir"
    Write-Host "Archived $ZipPath"
}

if ($Variant -in @("All", "Offline")) {
    Write-Host "Verifying offline portable dependency features"
    $RegistryFeatures = cargo tree -p typetext-desktop --target $WindowsTarget --no-default-features --features offline-portable -e features --locked | Select-String -Pattern "Win32_System_Registry|windows-startup-registry"
    if ($RegistryFeatures) {
        throw "Offline portable dependency graph unexpectedly includes Windows Registry support."
    }

    Write-Host "Building offline portable TypeText"
    cargo build --release --target $WindowsTarget -p typetext-desktop --no-default-features --features offline-portable --locked

    if (Test-Path $OfflineDistDir) {
        Remove-Item $OfflineDistDir -Recurse -Force
    }
    New-Item -ItemType Directory -Path $OfflineDataDir -Force | Out-Null
    Copy-Item $ExeSource $OfflineExeDest
    Invoke-TypeTextOptionalSigning -Path $OfflineExeDest

    if (Test-Path $SnippetsSource) {
        Copy-Item $SnippetsSource (Join-Path $OfflineDataDir "snippets.json")
    }
    if (Test-Path $OfflineSettingsSource) {
        Copy-Item $OfflineSettingsSource (Join-Path $OfflineDataDir "settings.json")
    }
    if (Test-Path $OfflineZipPath) {
        Remove-Item $OfflineZipPath -Force
    }
    Compress-Archive -Path $OfflineDistDir -DestinationPath $OfflineZipPath -Force
    Write-TypeTextSha256Checksum -Path $OfflineZipPath

    Write-Host "Built $OfflineDistDir"
    Write-Host "Archived $OfflineZipPath"
}
