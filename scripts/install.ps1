# Installs or updates Know Giver for the current user (no administrator
# rights), from the latest GitHub release:
#
#   irm https://raw.githubusercontent.com/czarist/know-giver/main/scripts/install.ps1 | iex
#
# Downloads the installer, checks it against the release's SHA256SUMS.txt
# and runs it silently. Your study data is not touched.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
# Windows PowerShell 5.1 may still offer TLS 1.0 first; GitHub needs 1.2.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
$repo = 'czarist/know-giver'
$base = "https://github.com/$repo/releases/latest/download"
$asset = 'KnowGiver-setup.exe'

# A clear message when no release is published yet (or it lacks the installer).
try {
    $latest = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -UseBasicParsing -Headers @{ 'User-Agent' = 'KnowGiver-installer' }
}
catch {
    throw "No Know Giver release is published yet. See https://github.com/$repo/releases"
}
if (-not ($latest.assets | Where-Object { $_.name -eq $asset })) {
    throw "Release $($latest.tag_name) has no $asset yet. See $($latest.html_url)"
}

$dir = Join-Path ([System.IO.Path]::GetTempPath()) ('KnowGiver-' + [guid]::NewGuid())
New-Item -ItemType Directory -Force $dir | Out-Null
try {
    $installer = Join-Path $dir $asset
    $sums = Join-Path $dir 'SHA256SUMS.txt'
    Write-Host "Downloading $asset"
    Invoke-WebRequest "$base/$asset" -OutFile $installer -UseBasicParsing
    Invoke-WebRequest "$base/SHA256SUMS.txt" -OutFile $sums -UseBasicParsing

    $expected = $null
    foreach ($line in Get-Content $sums) {
        if ($line -match '^\s*([0-9a-fA-F]{64})\s+\*?(\S+)\s*$' -and $Matches[2] -eq $asset) {
            $expected = $Matches[1].ToLowerInvariant()
        }
    }
    $actual = (Get-FileHash $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    if (-not $expected -or $expected -ne $actual) {
        throw "Checksum mismatch for $asset; nothing was installed."
    }

    $arguments = @('/SILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CLOSEAPPLICATIONS')
    # Optional install choices are asked once; updates keep their existing
    # startup entry and reminder preference. Environment variables answer in advance.
    $installed = Test-Path (Join-Path $env:LOCALAPPDATA 'Programs\KnowGiver\know-giver.exe')
    if (-not $installed) {
        $tasks = @()
        $answer = $env:KG_DESKTOP_SHORTCUT
        if (-not $answer) {
            try { $answer = Read-Host 'Create a desktop shortcut? [y/N]' } catch { $answer = '' }
        }
        if ($answer -match '^(1|y|s)') { $tasks += 'desktopicon' }

        $answer = $env:KG_STARTUP
        if (-not $answer) {
            try { $answer = Read-Host 'Start Know Giver when you sign in to Windows? [y/N]' } catch { $answer = '' }
        }
        if ($answer -match '^(1|y|s)') { $tasks += 'startup' }

        $answer = $env:KG_STUDY_REMINDERS
        if (-not $answer) {
            try { $answer = Read-Host 'Receive study reminders every 3 hours while Know Giver is running, including in the system tray? [y/N]' } catch { $answer = '' }
        }
        if ($answer -match '^(1|y|s)') { $tasks += 'studyreminders' }

        if ($tasks.Count -gt 0) { $arguments += "/MERGETASKS=$($tasks -join ',')" }

        # Claude through the student's own subscription needs Claude Code.
        $claude = Get-Command claude -ErrorAction SilentlyContinue
        $nativeClaude = Join-Path $env:USERPROFILE '.local\bin\claude.exe'
        if (-not $claude -and -not (Test-Path $nativeClaude)) {
            $answer = $env:KG_CLAUDE_CODE
            if (-not $answer) {
                try { $answer = Read-Host "Also install Claude Code (Anthropic's official program) to use your Claude subscription in Know Giver? [y/N]" } catch { $answer = '' }
            }
            $installClaude = $answer -match '^(1|y|s)'
        }
    }

    Write-Host 'Installing Know Giver'
    $process = Start-Process $installer -ArgumentList $arguments -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "The installer stopped with code $($process.ExitCode)." }
    Write-Host "Installed in $env:LOCALAPPDATA\Programs\KnowGiver"
    if ($installClaude) {
        Write-Host "Installing Claude Code with Anthropic's official installer"
        try {
            & powershell -NoProfile -ExecutionPolicy Bypass -Command 'irm https://claude.ai/install.ps1 | iex'
        } catch {
            Write-Host "Claude Code was not installed: $_. Know Giver can install it later from the AI step."
        }
    }
}
finally {
    Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction SilentlyContinue
}
