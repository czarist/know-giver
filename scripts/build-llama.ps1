$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    $llama = @{}
    $inLlamaSection = $false
    foreach ($line in Get-Content 'local-ai.toml') {
        if ($line -match '^\[(.+)\]$') {
            $inLlamaSection = $Matches[1] -eq 'llama_cpp'
            continue
        }
        if ($inLlamaSection -and $line -match '^(\w+) = "([^"]+)"$') {
            $llama[$Matches[1]] = $Matches[2]
        }
    }
    if (-not ($llama.version -and $llama.repository) -or $llama.commit -notmatch '^[0-9a-f]{40}$') {
        throw 'llama.cpp version, commit or repository is invalid in local-ai.toml'
    }

    $target = Join-Path $root 'target'
    $source = Join-Path $target 'local-ai-src\llama.cpp'
    $build = Join-Path $target 'local-ai-build'
    $runtime = Join-Path $target 'release\local-runtime'
    $cpuBuild = Join-Path $build 'cpu'
    $vulkanBuild = Join-Path $build 'vulkan'
    New-Item -ItemType Directory -Force (Split-Path $source -Parent) | Out-Null

    if (-not (Test-Path -LiteralPath (Join-Path $source '.git'))) {
        git clone --depth 1 --branch $llama.version $llama.repository $source
        if ($LASTEXITCODE -ne 0) { throw 'Could not clone the pinned llama.cpp release' }
    }
    $actualCommit = (git -C $source rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Could not read the llama.cpp checkout revision' }
    if ($actualCommit -ne $llama.commit) {
        throw "llama.cpp checkout does not match local-ai.toml: expected $($llama.commit), got $actualCommit"
    }

    if (-not $env:VULKAN_SDK -or -not (Test-Path -LiteralPath (Join-Path $env:VULKAN_SDK 'Bin\glslc.exe'))) {
        $sdkRoots = @('C:\VulkanSDK', (Join-Path $env:ProgramFiles 'VulkanSDK'))
        $sdk = $sdkRoots |
            Where-Object { Test-Path -LiteralPath $_ } |
            ForEach-Object { Get-ChildItem -LiteralPath $_ -Directory } |
            Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'Bin\glslc.exe') } |
            Sort-Object { [version]$_.Name } -Descending |
            Select-Object -First 1
        if ($sdk) { $env:VULKAN_SDK = $sdk.FullName }
    }
    if (-not $env:VULKAN_SDK -or -not (Test-Path -LiteralPath (Join-Path $env:VULKAN_SDK 'Bin\glslc.exe'))) {
        throw 'Install the Vulkan SDK and set VULKAN_SDK before building the Windows OCR runtimes'
    }

    $commonOptions = @(
        '-DBUILD_SHARED_LIBS=OFF',
        '-DGGML_NATIVE=OFF',
        '-DLLAMA_BUILD_TESTS=OFF',
        '-DLLAMA_BUILD_EXAMPLES=OFF',
        '-DLLAMA_BUILD_APP=OFF',
        '-DLLAMA_BUILD_TOOLS=ON',
        '-DLLAMA_BUILD_SERVER=ON',
        '-DLLAMA_BUILD_UI=OFF',
        '-DLLAMA_USE_PREBUILT_UI=OFF',
        '-DLLAMA_OPENSSL=OFF'
    )
    cmake -S $source -B $cpuBuild @commonOptions -DGGML_VULKAN=OFF
    if ($LASTEXITCODE -ne 0) { throw 'Could not configure the CPU llama.cpp runtime' }
    cmake --build $cpuBuild --config Release --target llama-server --parallel 2
    if ($LASTEXITCODE -ne 0) { throw 'Could not build the CPU llama-server' }

    cmake -S $source -B $vulkanBuild @commonOptions -DGGML_VULKAN=ON
    if ($LASTEXITCODE -ne 0) { throw 'Could not configure the Vulkan llama.cpp runtime' }
    cmake --build $vulkanBuild --config Release --target llama-server --parallel 2
    if ($LASTEXITCODE -ne 0) { throw 'Could not build the Vulkan llama-server' }

    $cpuServer = Get-ChildItem -LiteralPath (Join-Path $cpuBuild 'bin') -Filter 'llama-server.exe' -Recurse -File |
        Where-Object { $_.FullName -match '\\Release\\llama-server\.exe$' } |
        Select-Object -First 1
    if (-not $cpuServer) { throw 'The CPU llama.cpp build produced no Release llama-server.exe' }
    $vulkanServer = Get-ChildItem -LiteralPath (Join-Path $vulkanBuild 'bin') -Filter 'llama-server.exe' -Recurse -File |
        Where-Object { $_.FullName -match '\\Release\\llama-server\.exe$' } |
        Select-Object -First 1
    if (-not $vulkanServer) { throw 'The Vulkan llama.cpp build produced no Release llama-server.exe' }
    New-Item -ItemType Directory -Force $runtime | Out-Null
    New-Item -ItemType Directory -Force (Join-Path $runtime 'vulkan') | Out-Null
    Copy-Item -LiteralPath $cpuServer.FullName -Destination (Join-Path $runtime 'llama-server.exe') -Force
    Copy-Item -LiteralPath $vulkanServer.FullName -Destination (Join-Path $runtime 'vulkan\llama-server.exe') -Force
    Copy-Item -LiteralPath (Join-Path $source 'LICENSE') -Destination (Join-Path $runtime 'llama.cpp-LICENSE') -Force

    $vswhere = Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) {
        throw 'Could not find vswhere.exe to locate the Visual C++ runtime redistributables'
    }
    $vsInstall = (& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath).Trim()
    if ($LASTEXITCODE -ne 0 -or -not $vsInstall) {
        throw 'Could not locate the Visual Studio C++ build tools installation'
    }
    $redistRoot = Join-Path $vsInstall 'VC\Redist\MSVC'
    $redistVersion = Get-ChildItem -LiteralPath $redistRoot -Directory |
        Where-Object { $_.Name -match '^\d+(\.\d+)*$' } |
        Sort-Object { [version]$_.Name } -Descending |
        Where-Object {
            (Test-Path -LiteralPath (Join-Path $_.FullName 'x64\Microsoft.VC143.CRT')) -and
            (Test-Path -LiteralPath (Join-Path $_.FullName 'x64\Microsoft.VC143.OpenMP'))
        } |
        Select-Object -First 1
    if (-not $redistVersion) {
        throw 'Could not find the x64 Visual C++ CRT and OpenMP redistributables'
    }
    $crt = Join-Path $redistVersion.FullName 'x64\Microsoft.VC143.CRT'
    $openMp = Join-Path $redistVersion.FullName 'x64\Microsoft.VC143.OpenMP'
    $runtimeDlls = @{
        'msvcp140.dll' = Join-Path $crt 'msvcp140.dll'
        'vcruntime140.dll' = Join-Path $crt 'vcruntime140.dll'
        'vcruntime140_1.dll' = Join-Path $crt 'vcruntime140_1.dll'
        'vcomp140.dll' = Join-Path $openMp 'vcomp140.dll'
    }
    foreach ($dll in $runtimeDlls.Keys) {
        $sourceDll = $runtimeDlls[$dll]
        if (-not (Test-Path -LiteralPath $sourceDll)) {
            throw "Required Visual C++ runtime dependency is missing: $sourceDll"
        }
        Copy-Item -LiteralPath $sourceDll -Destination $runtime -Force
        Copy-Item -LiteralPath $sourceDll -Destination (Join-Path $runtime 'vulkan') -Force
    }

    $vulkanLoaderCandidates = @(
        (Join-Path $env:VULKAN_SDK 'Bin\vulkan-1.dll'),
        (Join-Path $env:windir 'System32\vulkan-1.dll')
    )
    $vulkanLoader = $vulkanLoaderCandidates |
        Where-Object { Test-Path -LiteralPath $_ } |
        Select-Object -First 1
    if (-not $vulkanLoader) {
        throw 'Could not find vulkan-1.dll to bundle with the Vulkan llama-server'
    }
    $vulkanLicense = Join-Path $env:VULKAN_SDK 'Licenses\LICENSE.txt'
    if (-not (Test-Path -LiteralPath $vulkanLicense)) {
        throw 'Could not find the Vulkan SDK license registry'
    }
    Copy-Item -LiteralPath $vulkanLoader -Destination (Join-Path $runtime 'vulkan\vulkan-1.dll') -Force
    Copy-Item -LiteralPath $vulkanLicense -Destination (Join-Path $runtime 'Vulkan-SDK-LICENSE.txt') -Force
}
finally {
    Pop-Location
}
