function Get-TypeTextVersion {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RootDir
    )

    if (![string]::IsNullOrWhiteSpace($env:TYPETEXT_VERSION)) {
        return $env:TYPETEXT_VERSION.Trim()
    }

    $Tag = (& git -C $RootDir describe --tags --exact-match 2>$null)
    if (![string]::IsNullOrWhiteSpace($Tag)) {
        return $Tag.Trim()
    }

    $VersionPath = Join-Path $RootDir "VERSION"
    if (Test-Path $VersionPath) {
        return (Get-Content $VersionPath -Raw).Trim()
    }

    $CargoVersion = Select-String -Path (Join-Path $RootDir "Cargo.toml") -Pattern '^version = "(.+)"' | Select-Object -First 1
    return $CargoVersion.Matches[0].Groups[1].Value
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

function Write-TypeTextMd5Checksum {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [string]$ChecksumPath
    )

    if ([string]::IsNullOrWhiteSpace($ChecksumPath)) {
        $ChecksumPath = "$Path.md5"
    }

    $Hash = Get-FileHash -Algorithm MD5 -Path $Path
    $FileName = Split-Path -Leaf $Path
    "$($Hash.Hash.ToLowerInvariant())  $FileName" | Set-Content -Path $ChecksumPath -Encoding ASCII
    Write-Host "Wrote $ChecksumPath"
}
