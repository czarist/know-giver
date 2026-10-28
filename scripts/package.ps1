# Builds a release and packages Know Giver for Windows.
#
#   powershell -ExecutionPolicy Bypass -File scripts\package.ps1
#
# Output in dist\:
#   KnowGiver-<version>-windows-x64.zip     portable build (always)
#   KnowGiver-<version>-setup.exe           installer (when Inno Setup 6 is installed)

param([switch]$SkipInstaller)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    $versionLine = Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1
    $version = $versionLine.Matches[0].Groups[1].Value

    cargo build --release -p know-giver
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

    $dist = Join-Path $root 'dist'
    $stage = [System.IO.Path]::GetFullPath((Join-Path $dist 'KnowGiver'))
    $expectedStage = [System.IO.Path]::GetFullPath((Join-Path $root 'dist\KnowGiver'))
    if ($stage -ne $expectedStage -or -not $stage.StartsWith([System.IO.Path]::GetFullPath($dist) + [System.IO.Path]::DirectorySeparatorChar)) {
        throw 'Refusing to clean a staging directory outside dist'
    }
    if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
    New-Item -ItemType Directory -Force $stage | Out-Null
    Copy-Item 'target\release\know-giver.exe' $stage
    Copy-Item '.env.example' (Join-Path $stage '.env.example')
    Copy-Item 'packaging\windows\LEIA-ME.txt' $stage

    $zip = Join-Path $dist "KnowGiver-$version-windows-x64.zip"
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip -Force
    Write-Host "Portable build: $zip"

    if ($SkipInstaller) { return }
    $iscc = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $iscc) {
        Write-Host 'Inno Setup 6 not found; skipping the installer (https://jrsoftware.org/isinfo.php).'
        return
    }
    & $iscc "/DAppVersion=$version" "/DSourceDir=$stage" "/DRepoRoot=$root" "/O$dist" 'packaging\windows\know-giver.iss'
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed" }
}
finally {
    Pop-Location
}
