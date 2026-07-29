param(
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Source,
    [Parameter(Mandatory = $true)][string]$OutputDir
)

$ErrorActionPreference = "Stop"
$RootDir = Split-Path -Parent $PSScriptRoot
$OutputDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDir)
$TemplatePath = Join-Path $RootDir "docs\user-guide.template.html"
$HtmlPath = Join-Path $OutputDir "TypeText-User-Guide.html"
$PdfPath = Join-Path $OutputDir "TypeText-User-Guide.pdf"

New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
$Template = Get-Content -Raw -LiteralPath $TemplatePath
$GuideSource = (Get-Content -Raw -LiteralPath $Source).Replace("{{VERSION}}", $Version)
$Html = $Template.Replace("{{VERSION}}", $Version).Replace("{{GUIDE_SOURCE}}", $GuideSource)
[System.IO.File]::WriteAllText($HtmlPath, $Html, [System.Text.UTF8Encoding]::new($false))

$BrowserCandidates = @(
    "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe",
    "${env:ProgramFiles}\Microsoft\Edge\Application\msedge.exe",
    "${env:ProgramFiles}\Google\Chrome\Application\chrome.exe",
    "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe"
)
$Browser = $BrowserCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (!$Browser) {
    throw "PDF generation requires Microsoft Edge or Google Chrome."
}

$ProfileDir = Join-Path ([System.IO.Path]::GetTempPath()) ("typetext-guide-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $ProfileDir | Out-Null
try {
    $HtmlUri = [uri]::new((Resolve-Path -LiteralPath $HtmlPath)).AbsoluteUri
    if (Test-Path -LiteralPath $PdfPath) {
        Remove-Item -LiteralPath $PdfPath -Force
    }
    $BrowserArguments = @(
        "--headless=new"
        "--disable-gpu"
        "--no-first-run"
        "--no-default-browser-check"
        "--no-pdf-header-footer"
        "--print-to-pdf-no-header"
        "--user-data-dir=$ProfileDir"
        "--print-to-pdf=$PdfPath"
        $HtmlUri
    )
    $BrowserProcess = Start-Process `
        -FilePath $Browser `
        -ArgumentList $BrowserArguments `
        -Wait `
        -PassThru `
        -WindowStyle Hidden
    if ($BrowserProcess.ExitCode -ne 0 -or !(Test-Path -LiteralPath $PdfPath)) {
        throw "Browser PDF generation failed. Browser: $Browser; exit code: $($BrowserProcess.ExitCode); expected output: $PdfPath"
    }
} finally {
    if (Test-Path -LiteralPath $ProfileDir) {
        Remove-Item -LiteralPath $ProfileDir -Recurse -Force
    }
}

Write-Host "Built $HtmlPath"
Write-Host "Built $PdfPath"
