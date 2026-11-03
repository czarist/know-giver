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

    cmake -S $source -B $build `
        -DBUILD_SHARED_LIBS=OFF `
        -DGGML_NATIVE=OFF `
        -DLLAMA_BUILD_TESTS=OFF `
        -DLLAMA_BUILD_EXAMPLES=OFF `
        -DLLAMA_BUILD_APP=OFF `
        -DLLAMA_BUILD_TOOLS=ON `
        -DLLAMA_BUILD_SERVER=ON `
        -DLLAMA_BUILD_UI=OFF `
        -DLLAMA_USE_PREBUILT_UI=OFF `
        -DLLAMA_OPENSSL=OFF
    if ($LASTEXITCODE -ne 0) { throw 'Could not configure llama.cpp' }
    cmake --build $build --config Release --target llama-server --parallel 2
    if ($LASTEXITCODE -ne 0) { throw 'Could not build llama-server' }

    $server = Get-ChildItem -LiteralPath (Join-Path $build 'bin') -Filter 'llama-server.exe' -Recurse -File |
        Where-Object { $_.FullName -match '\\Release\\llama-server\.exe$' } |
        Select-Object -First 1
    if (-not $server) { throw 'The llama.cpp build produced no Release llama-server.exe' }
    New-Item -ItemType Directory -Force $runtime | Out-Null
    Copy-Item -LiteralPath $server.FullName -Destination (Join-Path $runtime 'llama-server.exe') -Force
    Copy-Item -LiteralPath (Join-Path $source 'LICENSE') -Destination (Join-Path $runtime 'llama.cpp-LICENSE') -Force
}
finally {
    Pop-Location
}
