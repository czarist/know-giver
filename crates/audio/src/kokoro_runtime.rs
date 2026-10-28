//! Windows launches the pinned sherpa-onnx executable with a UTF-8 code page.
//!
//! That build uses `main(char **argv)`. Windows converts the Unicode command
//! line with the process active code page, so accented Portuguese arrives as
//! Windows-1252 and the runtime rejects it. A sibling copy gets an application
//! manifest that asks for UTF-8. The downloaded executable is left unchanged.

use std::path::{Path, PathBuf};

use crate::AudioError;

const ACTIVE_CODE_PAGE: &str = "<activeCodePage xmlns=\"http://schemas.microsoft.com/SMI/2019/WindowsSettings\">UTF-8</activeCodePage>";

/// Returns the executable that should receive Kokoro text.
///
/// On Windows this is a sibling whose manifest requests UTF-8. Elsewhere it is
/// `exe` unchanged.
///
/// # Errors
///
/// Returns an error when the Windows runtime cannot be copied or its manifest
/// cannot be updated. The original file is not modified.
pub fn prepare_kokoro_executable(exe: &Path) -> Result<PathBuf, AudioError> {
    #[cfg(windows)]
    {
        windows::prepare(exe)
    }
    #[cfg(not(windows))]
    {
        Ok(exe.to_path_buf())
    }
}

/// Inserts a UTF-8 active code page into an application manifest.
pub(crate) fn with_utf8_code_page(manifest: &str) -> Result<String, AudioError> {
    if has_utf8_code_page(manifest) {
        return Ok(manifest.to_owned());
    }
    let folded = manifest.to_ascii_lowercase();
    if let Some(index) = folded.rfind("</windowssettings>") {
        let mut updated = String::with_capacity(manifest.len() + ACTIVE_CODE_PAGE.len());
        updated.push_str(&manifest[..index]);
        updated.push_str(ACTIVE_CODE_PAGE);
        updated.push_str(&manifest[index..]);
        return Ok(updated);
    }
    let Some(index) = folded.rfind("</assembly>") else {
        return Err(AudioError::Tts(
            "Kokoro runtime manifest has no assembly element".into(),
        ));
    };
    let mut updated = String::with_capacity(manifest.len() + ACTIVE_CODE_PAGE.len() + 160);
    updated.push_str(&manifest[..index]);
    updated.push_str("\n  <application xmlns=\"urn:schemas-microsoft-com:asm.v3\">\n    <windowsSettings>\n      ");
    updated.push_str(ACTIVE_CODE_PAGE);
    updated.push_str("\n    </windowsSettings>\n  </application>\n");
    updated.push_str(&manifest[index..]);
    Ok(updated)
}

fn has_utf8_code_page(manifest: &str) -> bool {
    let folded = manifest.to_ascii_lowercase();
    let Some(start) = folded.find("<activecodepage") else {
        return false;
    };
    let Some(end) = folded[start..].find("</activecodepage>") else {
        return false;
    };
    folded[start..start + end].contains("utf-8")
}

fn decode_manifest(bytes: &[u8]) -> Result<String, AudioError> {
    let text = if bytes.starts_with(&[0xFF, 0xFE]) {
        if !bytes[2..].len().is_multiple_of(2) {
            return Err(AudioError::Tts(
                "Kokoro runtime manifest is not Unicode".into(),
            ));
        }
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|unit| u16::from_le_bytes(*unit))
            .collect();
        String::from_utf16(&units)
            .map_err(|_| AudioError::Tts("Kokoro runtime manifest is not Unicode".into()))?
    } else {
        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        String::from_utf8(bytes.to_vec())
            .map_err(|_| AudioError::Tts("Kokoro runtime manifest is not UTF-8".into()))?
    };
    Ok(text.trim_end_matches('\0').to_owned())
}

#[cfg(windows)]
mod windows {
    use std::ffi::OsStr;
    use std::fs;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::UNIX_EPOCH;

    use super::{decode_manifest, with_utf8_code_page};
    use crate::AudioError;

    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$exe = $env:KG_KOKORO_EXE
$mode = $env:KG_KOKORO_MODE
if (-not (Test-Path -LiteralPath $exe)) { throw 'missing Kokoro runtime' }
$b = [System.IO.File]::ReadAllBytes($exe)
if ($b.Length -lt 64 -or $b[0] -ne 77 -or $b[1] -ne 90) { throw 'Kokoro runtime is not a Windows executable' }
$pe = [BitConverter]::ToInt32($b, 0x3C)
if ($pe -le 0 -or ($pe + 24) -ge $b.Length) { throw 'Kokoro runtime has a truncated header' }
$magic = [BitConverter]::ToUInt16($b, $pe + 24)
if ($magic -eq 0x20b) { $dd = $pe + 24 + 112 } else { $dd = $pe + 24 + 96 }
if (($dd + 24) -gt $b.Length) { throw 'Kokoro runtime has no resource directory' }
$resRva = [BitConverter]::ToUInt32($b, $dd + 16)
if ($resRva -eq 0) { throw 'Kokoro runtime has no embedded manifest' }
$numSec = [BitConverter]::ToUInt16($b, $pe + 6)
$sizeOpt = [BitConverter]::ToUInt16($b, $pe + 20)
$sec = $pe + 24 + $sizeOpt
function ConvertTo-FileOffset([uint32]$rva) {
  for ($i = 0; $i -lt $numSec; $i++) {
    $o = $sec + ($i * 40)
    if (($o + 24) -gt $b.Length) { throw 'truncated section table' }
    $va = [BitConverter]::ToUInt32($b, $o + 12)
    $vs = [BitConverter]::ToUInt32($b, $o + 8)
    $raw = [BitConverter]::ToUInt32($b, $o + 20)
    if ($rva -ge $va -and $rva -lt ($va + $vs)) { return [uint32]($raw + ($rva - $va)) }
  }
  throw 'resource address is outside the Kokoro runtime'
}
function Find-Manifest {
  param([uint32]$dirOff, [int]$depth, [uint32]$id)
  $named = [BitConverter]::ToUInt16($b, $dirOff + 12)
  $ids = [BitConverter]::ToUInt16($b, $dirOff + 14)
  $count = $named + $ids
  $entry = $dirOff + 16
  for ($i = 0; $i -lt $count; $i++) {
    $e = $entry + ($i * 8)
    if (($e + 8) -gt $b.Length) { throw 'truncated resource directory' }
    $name = [BitConverter]::ToUInt32($b, $e)
    $data = [BitConverter]::ToUInt32($b, $e + 4)
    $entryId = $name -band 0xFFFF
    $next = $data -band 0x7FFFFFFF
    $sub = ($data -band 0x80000000) -ne 0
    $selected = ($depth -eq 2) -or ($entryId -eq $id)
    if (-not $selected) { continue }
    if ($sub) {
      $child = (ConvertTo-FileOffset $resRva) + $next
      $found = Find-Manifest $child ($depth + 1) $(if ($depth -eq 0) { 1 } else { $id })
      if ($null -ne $found) { return $found }
    } elseif ($depth -eq 2) {
      $leaf = (ConvertTo-FileOffset $resRva) + $next
      return @{
        Language = [uint16]$entryId
        Offset = (ConvertTo-FileOffset ([BitConverter]::ToUInt32($b, $leaf)))
        Size = [BitConverter]::ToUInt32($b, $leaf + 4)
      }
    }
  }
  return $null
}
$root = ConvertTo-FileOffset $resRva
$manifest = Find-Manifest $root 0 24
if ($null -eq $manifest) { throw 'Kokoro runtime has no embedded manifest' }
if ($manifest.Offset + $manifest.Size -gt $b.Length) { throw 'embedded manifest is truncated' }
if ($mode -eq 'extract') {
  $slice = New-Object byte[] $manifest.Size
  [Array]::Copy($b, $manifest.Offset, $slice, 0, $manifest.Size)
  [System.IO.File]::WriteAllBytes($env:KG_MANIFEST_OUT, $slice)
  exit 0
}
if ($mode -ne 'update') { throw 'unknown Kokoro manifest mode' }
$payload = [System.IO.File]::ReadAllBytes($env:KG_MANIFEST_IN)
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class KgKokoroManifest {
  [DllImport("kernel32", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern IntPtr BeginUpdateResource(string file, bool deleteExisting);
  [DllImport("kernel32", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern bool UpdateResource(IntPtr handle, IntPtr type, IntPtr name, ushort language, byte[] data, uint size);
  [DllImport("kernel32", SetLastError=true)]
  public static extern bool EndUpdateResource(IntPtr handle, bool discard);
}
'@
$handle = [KgKokoroManifest]::BeginUpdateResource($exe, $false)
if ($handle -eq [IntPtr]::Zero) { throw "could not open the Kokoro runtime for update ($([Runtime.InteropServices.Marshal]::GetLastWin32Error()))" }
$ok = [KgKokoroManifest]::UpdateResource($handle, [IntPtr]24, [IntPtr]1, $manifest.Language, $payload, [uint32]$payload.Length)
if (-not $ok) {
  $code = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
  [KgKokoroManifest]::EndUpdateResource($handle, $true) | Out-Null
  throw "could not update the Kokoro runtime manifest ($code)"
}
if (-not [KgKokoroManifest]::EndUpdateResource($handle, $false)) {
  throw "could not save the Kokoro runtime manifest ($([Runtime.InteropServices.Marshal]::GetLastWin32Error()))"
}
exit 0
"#;

    pub(super) fn prepare(exe: &Path) -> Result<PathBuf, AudioError> {
        let exe = fs::canonicalize(exe).map_err(|error| AudioError::io(exe, error))?;
        let dest = patched_path(&exe);
        let stamp = dest.with_extension("stamp");
        if stamp_matches(&exe, &dest, &stamp) {
            return Ok(dest);
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|error| AudioError::io(parent, error))?;
        }
        let partial = dest.with_extension("partial");
        let _ = fs::remove_file(&partial);
        fs::copy(&exe, &partial).map_err(|error| AudioError::io(&partial, error))?;
        let result = patch_manifest(&partial);
        if let Err(error) = result {
            let _ = fs::remove_file(&partial);
            return Err(error);
        }
        let _ = fs::remove_file(&dest);
        fs::rename(&partial, &dest).map_err(|error| AudioError::io(&dest, error))?;
        let meta = fs::metadata(&exe).map_err(|error| AudioError::io(&exe, error))?;
        fs::write(&stamp, stamp_text(&meta)).map_err(|error| AudioError::io(&stamp, error))?;
        tracing::info!(runtime = %dest.display(), "prepared Kokoro runtime with a UTF-8 code page");
        Ok(dest)
    }

    fn patched_path(exe: &Path) -> PathBuf {
        let mut name = exe.file_stem().unwrap_or_default().to_os_string();
        name.push(".utf8.exe");
        exe.with_file_name(name)
    }

    fn stamp_matches(exe: &Path, dest: &Path, stamp: &Path) -> bool {
        let Ok(meta) = fs::metadata(exe) else {
            return false;
        };
        let Ok(saved) = fs::read_to_string(stamp) else {
            return false;
        };
        dest.is_file() && saved == stamp_text(&meta)
    }

    fn stamp_text(meta: &fs::Metadata) -> String {
        let modified = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|time| time.as_secs())
            .unwrap_or(0);
        format!("{}\n{modified}\n", meta.len())
    }

    fn patch_manifest(exe: &Path) -> Result<(), AudioError> {
        // Close our handle first. PowerShell cannot read a script or manifest
        // that this process still has open.
        let extracted = closed_temp("")?;
        powershell(&[
            ("KG_KOKORO_MODE", OsStr::new("extract")),
            ("KG_KOKORO_EXE", exe.as_os_str()),
            ("KG_MANIFEST_OUT", extracted.as_os_str()),
        ])?;
        let bytes = fs::read(&extracted).map_err(|error| AudioError::io(&extracted, error))?;
        let updated = with_utf8_code_page(&decode_manifest(&bytes)?)?;
        let payload = closed_temp("")?;
        fs::write(&payload, updated.as_bytes()).map_err(|error| AudioError::io(&payload, error))?;
        powershell(&[
            ("KG_KOKORO_MODE", OsStr::new("update")),
            ("KG_KOKORO_EXE", exe.as_os_str()),
            ("KG_MANIFEST_IN", payload.as_os_str()),
        ])?;
        powershell(&[
            ("KG_KOKORO_MODE", OsStr::new("extract")),
            ("KG_KOKORO_EXE", exe.as_os_str()),
            ("KG_MANIFEST_OUT", extracted.as_os_str()),
        ])?;
        let bytes = fs::read(&extracted).map_err(|error| AudioError::io(&extracted, error))?;
        let manifest = decode_manifest(&bytes)?;
        if !manifest.contains("activeCodePage") || !manifest.to_ascii_lowercase().contains("utf-8")
        {
            return Err(AudioError::Tts(
                "Kokoro runtime manifest did not accept a UTF-8 code page".into(),
            ));
        }
        if !manifest.contains("asInvoker") {
            return Err(AudioError::Tts(
                "Kokoro runtime manifest lost its execution level".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn closed_temp(suffix: &str) -> Result<tempfile::TempPath, AudioError> {
        let file = tempfile::Builder::new()
            .suffix(suffix)
            .tempfile()
            .map_err(|error| {
                AudioError::Tts(format!("could not prepare Kokoro runtime: {error}"))
            })?;
        Ok(file.into_temp_path())
    }

    pub(super) fn powershell(vars: &[(&str, &OsStr)]) -> Result<(), AudioError> {
        let file = closed_temp(".ps1")?;
        fs::write(&file, SCRIPT).map_err(|error| AudioError::io(&file, error))?;
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ]);
        command.arg(&file);
        command.envs(vars.iter().copied());
        command.stdin(std::process::Stdio::null());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        // CREATE_NO_WINDOW. Manifest preparation is a one-time child process.
        command.creation_flags(0x0800_0000);
        let output = command.output().map_err(|error| {
            AudioError::Tts(format!("could not prepare Kokoro runtime: {error}"))
        })?;
        if output.status.success() {
            return Ok(());
        }
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.trim();
        let detail = if detail.len() > 500 {
            &detail[..500]
        } else {
            detail
        };
        Err(AudioError::Tts(format!(
            "could not prepare Kokoro runtime ({}){}",
            output.status,
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PINNED: &str = "<?xml version='1.0' encoding='UTF-8' standalone='yes'?>
<assembly xmlns='urn:schemas-microsoft-com:asm.v1' manifestVersion='1.0'>
  <trustInfo xmlns=\"urn:schemas-microsoft-com:asm.v3\">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level='asInvoker' uiAccess='false' />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
";

    #[test]
    fn pinned_manifest_gains_utf8_without_losing_its_trust_info() {
        let updated = with_utf8_code_page(PINNED).unwrap();
        assert!(updated.contains("asInvoker"));
        assert!(updated.contains(ACTIVE_CODE_PAGE));
        assert!(updated.contains("</assembly>"));
        assert_eq!(with_utf8_code_page(&updated).unwrap(), updated);
    }

    #[test]
    fn existing_windows_settings_receive_the_code_page() {
        let manifest =
            "<assembly><windowsSettings><dpiAware>true</dpiAware></windowsSettings></assembly>";
        let updated = with_utf8_code_page(manifest).unwrap();
        assert!(updated.contains("dpiAware"));
        assert_eq!(updated.matches("<application").count(), 0);
        assert!(updated.contains("UTF-8"));
    }

    #[test]
    fn manifest_without_an_assembly_is_rejected() {
        let error = with_utf8_code_page("<trustInfo></trustInfo>").unwrap_err();
        assert!(error.to_string().contains("assembly"), "{error}");
    }

    #[cfg(windows)]
    #[test]
    fn pinned_runtime_copy_requests_a_utf8_code_page() {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "../../target/voice-verification/sherpa-onnx-v1.12.20-win-x64-shared/bin/sherpa-onnx-offline-tts.exe",
        );
        if !source.is_file() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("sherpa-onnx-offline-tts.exe");
        std::fs::copy(&source, &exe).unwrap();
        let prepared = prepare_kokoro_executable(&exe).unwrap();
        assert_ne!(prepared, exe);
        let again = prepare_kokoro_executable(&exe).unwrap();
        assert_eq!(prepared, again);
        let manifest = windows::closed_temp("").unwrap();
        windows::powershell(&[
            ("KG_KOKORO_MODE", std::ffi::OsStr::new("extract")),
            ("KG_KOKORO_EXE", prepared.as_os_str()),
            ("KG_MANIFEST_OUT", manifest.as_os_str()),
        ])
        .unwrap();
        let text = decode_manifest(&std::fs::read(&manifest).unwrap()).unwrap();
        assert!(text.contains("UTF-8"), "{text}");
        assert!(text.contains("asInvoker"), "{text}");
    }
}
