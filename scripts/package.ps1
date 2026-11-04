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

    & (Join-Path $PSScriptRoot 'build-llama.ps1')
    if ($LASTEXITCODE -ne 0) { throw "llama.cpp build failed" }
    cargo build --release -p know-giver -j 2
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
    Copy-Item 'target\release\local-runtime' $stage -Recurse
    foreach ($dll in @('vcruntime140.dll', 'vcruntime140_1.dll')) {
        $runtimeDll = Join-Path 'target\release\local-runtime' $dll
        if (-not (Test-Path -LiteralPath $runtimeDll)) {
            throw "The packaged Windows build is missing the app runtime dependency $dll"
        }
        Copy-Item -LiteralPath $runtimeDll -Destination $stage -Force
    }
    Copy-Item '.env.example' (Join-Path $stage '.env.example')
    Copy-Item 'packaging\windows\README.*.txt' $stage
    if (-not (Test-Path -LiteralPath (Join-Path $stage 'local-runtime\vulkan\llama-server.exe'))) {
        throw 'The packaged Windows build is missing the Vulkan llama-server'
    }
    foreach ($dll in @('msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll', 'vcomp140.dll')) {
        if (-not (Test-Path -LiteralPath (Join-Path $stage "local-runtime\$dll"))) {
            throw "The packaged Windows build is missing the CPU runtime dependency $dll"
        }
        if (-not (Test-Path -LiteralPath (Join-Path $stage "local-runtime\vulkan\$dll"))) {
            throw "The packaged Windows build is missing the Vulkan runtime dependency $dll"
        }
    }
    if (-not (Test-Path -LiteralPath (Join-Path $stage 'local-runtime\vulkan\vulkan-1.dll'))) {
        throw 'The packaged Windows build is missing the Vulkan loader'
    }
    if (-not (Test-Path -LiteralPath (Join-Path $stage 'local-runtime\Vulkan-SDK-LICENSE.txt'))) {
        throw 'The packaged Windows build is missing the Vulkan SDK license registry'
    }

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
