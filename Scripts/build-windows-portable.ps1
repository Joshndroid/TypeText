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
$TokensSource = Join-Path $RootDir "examples\tokens.json"

function Invoke-TypeTextCargo {
    param(
        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]]$CargoArgs
    )

    & cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($CargoArgs -join ' ') failed with exit code $LASTEXITCODE."
    }
}

function Assert-TypeTextBuiltExecutable {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (!(Test-Path $Path)) {
        throw "Expected build output was not found: $Path"
    }
}

function Find-TypeTextDumpbin {
    $Command = Get-Command "dumpbin.exe" -ErrorAction SilentlyContinue
    if ($Command) {
        return $Command.Source
    }

    $VsWhereCandidates = @(
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe",
        "${env:ProgramFiles}\Microsoft Visual Studio\Installer\vswhere.exe"
    )
    foreach ($VsWhere in $VsWhereCandidates) {
        if (!(Test-Path $VsWhere)) {
            continue
        }

        $FoundPaths = @(
            & $VsWhere `
                -latest `
                -products "*" `
                -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
                -find "VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe"
        )
        $Dumpbin = $FoundPaths | Where-Object { Test-Path $_ } | Select-Object -First 1
        if ($Dumpbin) {
            return $Dumpbin
        }
    }

    throw "dumpbin.exe was not found. Install the Visual C++ x64 build tools so Windows PE security properties can be verified."
}

function Assert-TypeTextPeHardening {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $Dumpbin = Find-TypeTextDumpbin
    $DumpbinOutput = & $Dumpbin /nologo /headers $Path 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "dumpbin failed to inspect PE headers for $Path with exit code $LASTEXITCODE. $($DumpbinOutput | Out-String)"
    }
    $Headers = $DumpbinOutput -join "`n"

    $RequiredCharacteristics = @(
        "Dynamic base",
        "High Entropy Virtual Addresses",
        "NX compatible"
    )
    foreach ($Characteristic in $RequiredCharacteristics) {
        if ($Headers -notmatch "(?im)^\s+$([regex]::Escape($Characteristic))\s*$") {
            throw "$Path is missing required PE security characteristic: $Characteristic"
        }
    }

    $SectionMatches = [regex]::Matches(
        $Headers,
        '(?ms)^SECTION HEADER #[0-9]+\s*\r?\n\s+(\S+)\s+name\s*\r?\n(.*?)(?=^SECTION HEADER #[0-9]+|\z)'
    )
    if ($SectionMatches.Count -eq 0) {
        throw "Could not parse PE section headers for $Path; refusing to accept an unverified binary."
    }

    $ExecutableSections = @(
        $SectionMatches | Where-Object {
            $_.Groups[2].Value -match '(?im)^\s+.*\bExecute\b.*$'
        } | ForEach-Object {
            $_.Groups[1].Value
        }
    )
    if ($ExecutableSections.Count -ne 1 -or $ExecutableSections[0] -ne ".text") {
        $Description = if ($ExecutableSections.Count -eq 0) {
            "none"
        } else {
            $ExecutableSections -join ", "
        }
        throw "$Path has unexpected executable PE sections: $Description. Expected only .text."
    }

    # Latin-1 preserves byte values one-to-one, so the printable-ASCII regex
    # cannot bridge arbitrary high bytes by converting them to '?'.
    $BinaryText = [Text.Encoding]::GetEncoding(28591).GetString([IO.File]::ReadAllBytes($Path))
    $PdbReferences = @(
        [regex]::Matches($BinaryText, '[ -~]{4,}') | ForEach-Object {
            $_.Value
        } | Where-Object {
            $_ -match '(?i)\.pdb'
        }
    )
    $EmbeddedPdbPaths = @($PdbReferences | Where-Object { $_ -match '[\\/]' })
    if ($EmbeddedPdbPaths.Count -gt 0) {
        throw "$Path embeds a local PDB path: $($EmbeddedPdbPaths -join ', ')"
    }

    Write-Host "Verified PE hardening for $Path (ASLR, high-entropy VA, DEP/NX, .text-only execution, no PDB paths)."
    if ($PdbReferences.Count -gt 0) {
        Write-Host "Allowed filename-only PDB reference: $($PdbReferences -join ', ')"
    }
}

function Assert-TypeTextOfflineImports {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $Dumpbin = Find-TypeTextDumpbin
    $DumpbinOutput = & $Dumpbin /nologo /imports $Path 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "dumpbin failed to inspect $Path with exit code $LASTEXITCODE. $($DumpbinOutput | Out-String)"
    }

    $Imports = [regex]::Matches(
        ($DumpbinOutput -join "`n"),
        '(?im)^\s+([A-Za-z0-9_.-]+\.dll)\s*$'
    ) | ForEach-Object {
        $_.Groups[1].Value.ToLowerInvariant()
    } | Sort-Object -Unique

    if (!$Imports -or $Imports -notcontains "kernel32.dll") {
        throw "Could not parse the PE import table for $Path; refusing to accept an unverified offline binary."
    }

    $ForbiddenNetworkDlls = @(
        "dhcpcsvc.dll",
        "dhcpcsvc6.dll",
        "dnsapi.dll",
        "fwpuclnt.dll",
        "httpapi.dll",
        "iphlpapi.dll",
        "mpr.dll",
        "netapi32.dll",
        "rasapi32.dll",
        "rasdlg.dll",
        "rasman.dll",
        "urlmon.dll",
        "webio.dll",
        "wlanapi.dll",
        "winhttp.dll",
        "wininet.dll",
        "winhttpcom.dll",
        "ws2_32.dll",
        "wsock32.dll"
    )
    $UnexpectedImports = @($Imports | Where-Object { $_ -in $ForbiddenNetworkDlls })
    if ($UnexpectedImports.Count -gt 0) {
        throw "Offline portable binary unexpectedly imports direct networking DLLs: $($UnexpectedImports -join ', ')"
    }

    Write-Host "Verified offline PE imports contain no direct networking DLLs."
    Write-Host "Offline PE imports: $($Imports -join ', ')"
}

Set-Location $RootDir
Write-Host "Building TypeText for Windows target: $WindowsTarget"
Write-Host "Version: $Version"
Write-Host "Variant: $Variant"
Write-Host "If the target is missing, run: rustup target add $WindowsTarget"

if ($Variant -in @("All", "Standard")) {
    if (Test-Path $ExeSource) {
        Remove-Item $ExeSource -Force
    }
    Invoke-TypeTextCargo -CargoArgs @(
        "build",
        "--release",
        "--target",
        $WindowsTarget,
        "-p",
        "typetext-desktop",
        "--locked"
    )
    Assert-TypeTextBuiltExecutable -Path $ExeSource
    Assert-TypeTextPeHardening -Path $ExeSource

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
    if (Test-Path $TokensSource) {
        Copy-Item $TokensSource (Join-Path $DataDir "tokens.json")
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
    $TreeArgs = @(
        "tree",
        "-p",
        "typetext-desktop",
        "--target",
        $WindowsTarget,
        "--no-default-features",
        "--features",
        "offline-portable",
        "-e",
        "features",
        "--locked"
    )
    $TreeOutput = & cargo @TreeArgs 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($TreeArgs -join ' ') failed with exit code $LASTEXITCODE. $($TreeOutput | Out-String)"
    }

    $RegistryFeatures = $TreeOutput | Select-String -Pattern "Win32_System_Registry|windows-startup-registry"
    if ($RegistryFeatures) {
        throw "Offline portable dependency graph unexpectedly includes Windows Registry support."
    }

    Write-Host "Building offline portable TypeText"
    if (Test-Path $ExeSource) {
        Remove-Item $ExeSource -Force
    }
    Invoke-TypeTextCargo -CargoArgs @(
        "build",
        "--release",
        "--target",
        $WindowsTarget,
        "-p",
        "typetext-desktop",
        "--no-default-features",
        "--features",
        "offline-portable",
        "--locked"
    )
    Assert-TypeTextBuiltExecutable -Path $ExeSource
    Assert-TypeTextPeHardening -Path $ExeSource

    Write-Host "Verifying offline portable PE imports"
    Assert-TypeTextOfflineImports -Path $ExeSource

    Write-Host "Verifying offline portable binary capability markers"
    $OfflineBinaryText = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($ExeSource))
    $ForbiddenMarkers = @(
        "api.github.com/repos/Joshndroid/TypeText/releases/latest",
        "Could not open a WinHTTP session",
        "Only HTTPS links can be opened",
        "Software\Microsoft\Windows\CurrentVersion\Run"
    )
    foreach ($Marker in $ForbiddenMarkers) {
        if ($OfflineBinaryText.Contains($Marker)) {
            throw "Offline portable binary unexpectedly contains disabled capability marker: $Marker"
        }
    }

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
    if (Test-Path $TokensSource) {
        Copy-Item $TokensSource (Join-Path $OfflineDataDir "tokens.json")
    }
    if (Test-Path $OfflineZipPath) {
        Remove-Item $OfflineZipPath -Force
    }
    Compress-Archive -Path $OfflineDistDir -DestinationPath $OfflineZipPath -Force
    Write-TypeTextSha256Checksum -Path $OfflineZipPath

    Write-Host "Built $OfflineDistDir"
    Write-Host "Archived $OfflineZipPath"
}
