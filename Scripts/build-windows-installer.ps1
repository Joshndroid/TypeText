$ErrorActionPreference = "Stop"

$RootDir = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "version.ps1")
$PortableScript = Join-Path $PSScriptRoot "build-windows-portable.ps1"
$DistDir = Join-Path $RootDir "dist\TypeText-Windows"
$InstallerDir = Join-Path $RootDir "dist\installer-windows"
$OutputDir = Join-Path $RootDir "dist"
$IssPath = Join-Path $InstallerDir "TypeText.iss"
$Version = Get-TypeTextVersion -RootDir $RootDir
$PackageVersion = Get-TypeTextPackageVersion -Version $Version
$env:TYPETEXT_VERSION = $Version

& $PortableScript
$ExePath = Join-Path $DistDir "TypeText.exe"

if (Test-Path $InstallerDir) {
    Remove-Item $InstallerDir -Recurse -Force
}
New-Item -ItemType Directory -Path $InstallerDir -Force | Out-Null

$Iscc = $env:ISCC_EXE
if ([string]::IsNullOrWhiteSpace($Iscc)) {
    $Candidates = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles}\Inno Setup 6\ISCC.exe"
    )
    $Iscc = $Candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}

if ([string]::IsNullOrWhiteSpace($Iscc) -or !(Test-Path $Iscc)) {
    throw "Inno Setup compiler not found. Install Inno Setup 6 or set ISCC_EXE."
}

$DistDirForInno = $DistDir.Replace("\", "\\")
$OutputDirForInno = $OutputDir.Replace("\", "\\")
$IconPath = (Join-Path $RootDir "icon\TypeText.ico").Replace("\", "\\")

$InnoScript = @"
#define MyAppName "TypeText"
#define MyAppVersion "$PackageVersion"
#define MyAppPublisher "TypeText"
#define MyAppExeName "TypeText.exe"

[Setup]
AppId={{7D8E72A7-6E72-4B3B-9E50-7E03C792D98F}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
OutputDir=$OutputDirForInno
OutputBaseFilename=TypeText-Windows-x64-Setup
Compression=zip
SolidCompression=no
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile=$IconPath
UninstallDisplayIcon={app}\{#MyAppExeName}

[Files]
Source: "$DistDirForInno\\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall skipifsilent
"@

$InnoScript | Set-Content -Path $IssPath -Encoding UTF8
& $Iscc $IssPath

$SetupPath = Join-Path $OutputDir "TypeText-Windows-x64-Setup.exe"
Invoke-TypeTextOptionalSigning -Path $SetupPath
Write-TypeTextSha256Checksum -Path $SetupPath

Write-Host "Built $SetupPath"
