function Get-TypeTextVersion {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RootDir
    )

    $CargoVersion = Select-String -Path (Join-Path $RootDir "Cargo.toml") -Pattern '^version = "(.+)"' | Select-Object -First 1
    return "v$($CargoVersion.Matches[0].Groups[1].Value)"
}

function Get-TypeTextPackageVersion {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Version
    )

    if ($Version.StartsWith("v")) {
        return $Version.Substring(1)
    }
    return $Version
}

function Write-TypeTextSha256Checksum {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [string]$ChecksumPath
    )

    if ([string]::IsNullOrWhiteSpace($ChecksumPath)) {
        $ChecksumPath = "$Path.sha256"
    }

    $Hash = Get-FileHash -Algorithm SHA256 -Path $Path
    $FileName = Split-Path -Leaf $Path
    "$($Hash.Hash.ToLowerInvariant())  $FileName" | Set-Content -Path $ChecksumPath -Encoding ASCII
    Write-Host "Wrote $ChecksumPath"
}

function Write-TypeTextWindowsChecksumManifest {
    param(
        [Parameter(Mandatory = $true)]
        [string]$StandardArchivePath,

        [Parameter(Mandatory = $true)]
        [string]$OfflineArchivePath,

        [Parameter(Mandatory = $true)]
        [string]$InstallerPath,

        [Parameter(Mandatory = $true)]
        [string]$StandardExecutablePath,

        [Parameter(Mandatory = $true)]
        [string]$OfflineExecutablePath,

        [Parameter(Mandatory = $true)]
        [string]$OutputPath
    )

    $Entries = @(
        [PSCustomObject]@{ Path = $StandardArchivePath; Name = "TypeText-Windows-x64.zip" },
        [PSCustomObject]@{ Path = $OfflineArchivePath; Name = "TypeText-Windows-x64-Offline-Portable.zip" },
        [PSCustomObject]@{ Path = $InstallerPath; Name = "TypeText-Windows-x64-Setup.exe" },
        [PSCustomObject]@{ Path = $StandardExecutablePath; Name = "TypeText-Windows/TypeText.exe" },
        [PSCustomObject]@{ Path = $OfflineExecutablePath; Name = "TypeText-Windows-Offline-Portable/TypeText.exe" }
    )

    $Lines = foreach ($Entry in $Entries) {
        if (!(Test-Path -LiteralPath $Entry.Path -PathType Leaf)) {
            throw "Cannot hash missing Windows release file: $($Entry.Path)"
        }
        $Hash = Get-FileHash -Algorithm SHA256 -LiteralPath $Entry.Path
        "$($Hash.Hash.ToLowerInvariant())  $($Entry.Name)"
    }

    $Lines | Set-Content -LiteralPath $OutputPath -Encoding ASCII
    Write-Host "Wrote $OutputPath"
}

function Invoke-TypeTextOptionalSigning {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if ([string]::IsNullOrWhiteSpace($env:TYPETEXT_SIGNTOOL_COMMAND)) {
        return
    }

    if (!(Test-Path $Path)) {
        throw "Cannot sign missing file: $Path"
    }

    $Command = $env:TYPETEXT_SIGNTOOL_COMMAND.Replace("{file}", $Path)
    if ($Command -eq $env:TYPETEXT_SIGNTOOL_COMMAND) {
        $Command = "$Command `"$Path`""
    }

    Write-Host "Signing $Path"
    & cmd.exe /d /s /c $Command
    if ($LASTEXITCODE -ne 0) {
        throw "Signing failed for $Path"
    }
}
