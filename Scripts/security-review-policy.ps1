# Shared binary-policy definitions used by Windows packaging and the private
# behavioral security-review harness. The standard-build positive control
# prevents these markers from becoming stale, vacuous offline checks.

function Get-TypeTextOfflineCapabilityMarkers {
    return @(
        "api.github.com/repos/Joshndroid/TypeText/releases/latest",
        "Refusing to open an untrusted update URL",
        "Software\Microsoft\Windows\CurrentVersion\Run",
        "Windows startup entry creation failed:"
    )
}

function Get-TypeTextBinaryAsciiText {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )
    return [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($Path))
}

function Assert-TypeTextCapabilityMarkers {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [ValidateSet("Present", "Absent")]
        [string]$Expected
    )

    $binaryText = Get-TypeTextBinaryAsciiText -Path $Path
    foreach ($marker in Get-TypeTextOfflineCapabilityMarkers) {
        $present = $binaryText.Contains($marker)
        if ($Expected -eq "Present" -and -not $present) {
            throw "Standard binary positive control is missing capability marker: $marker"
        }
        if ($Expected -eq "Absent" -and $present) {
            throw "Offline portable binary unexpectedly contains disabled capability marker: $marker"
        }
    }
}
