//! Replaces the running program with one the GUI sent, then starts it. The new program must
//! answer `--version` before it is put in place, so a download for the wrong platform or a
//! damaged file cannot leave the target without an agent.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Set for the successor, which waits for its predecessor to release the instance port.
pub const RESTARTED: &str = "COMETDESK_AGENT_RESTARTED";

/// Puts `staged` where the running program is; returns that path (read it before the swap:
/// Linux reports a replaced program as `... (deleted)`).
pub fn install(staged: &Path) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find the agent program: {e}"))?;
    let name = exe
        .file_name()
        .ok_or("the agent program has no name")?
        .to_string_lossy()
        .into_owned();
    let new = exe.with_file_name(format!("{name}.new"));
    std::fs::copy(staged, &new).map_err(|e| format!("cannot write {}: {e}", new.display()))?;
    let _ = std::fs::remove_file(staged);
    let swapped = prepare(&new, &exe).and_then(|()| swap(&exe, &new));
    if swapped.is_err() {
        let _ = std::fs::remove_file(&new);
    }
    swapped.map(|()| exe)
}

fn prepare(new: &Path, exe: &Path) -> Result<(), String> {
    verify_candidate(new, exe)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(new, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    check(new)
}

fn verify_candidate(new: &Path, exe: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(new)
        .map_err(|e| format!("cannot inspect update candidate: {e}"))?;
    if !meta.file_type().is_file() {
        return Err("update candidate is not a regular file".into());
    }
    let size = meta.len();
    if !(64 * 1024..=100 * 1024 * 1024).contains(&size) {
        return Err(format!("update candidate size is invalid: {size} bytes"));
    }

    let mut header = [0u8; 64];
    let mut file =
        std::fs::File::open(new).map_err(|e| format!("cannot open update candidate: {e}"))?;
    use std::io::Read;
    file.read_exact(&mut header)
        .map_err(|e| format!("cannot read update header: {e}"))?;
    if header.starts_with(b"#!") {
        return Err("update candidate is a script, not an executable binary".into());
    }

    verify_format(&header, new, exe)?;

    let has_signature = binary_contains(new, b"cometdesk-agent")
        .map_err(|e| format!("cannot scan update candidate: {e}"))?;
    if !has_signature {
        return Err("update candidate does not match cometdesk-agent signature".into());
    }

    Ok(())
}

fn verify_format(new_header: &[u8; 64], _new: &Path, exe: &Path) -> Result<(), String> {
    use std::io::Read;
    let mut exe_header = [0u8; 64];
    if let Ok(mut f) = std::fs::File::open(exe) {
        let _ = f.read_exact(&mut exe_header);
    }

    #[cfg(target_os = "linux")]
    {
        if &new_header[0..4] != b"\x7fELF" {
            return Err("update candidate is not an ELF binary".into());
        }
        if exe_header.starts_with(b"\x7fELF") {
            if new_header[4] != exe_header[4] || new_header[5] != exe_header[5] {
                return Err("update candidate architecture/endianness does not match".into());
            }
            if new_header[18..20] != exe_header[18..20] {
                return Err("update candidate target machine does not match".into());
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let is_macho = |h: &[u8; 64]| {
            let magic = u32::from_ne_bytes([h[0], h[1], h[2], h[3]]);
            matches!(
                magic,
                0xFEEDFACF
                    | 0xCFFAEDFE
                    | 0xFEEDFACE
                    | 0xCEFAEDFE
                    | 0xCAFEBABE
                    | 0xBEBAFECA
                    | 0xCAFEBABF
            )
        };
        if !is_macho(new_header) {
            return Err("update candidate is not a Mach-O binary".into());
        }
        if is_macho(&exe_header) {
            let magic_new =
                u32::from_ne_bytes([new_header[0], new_header[1], new_header[2], new_header[3]]);
            let magic_exe =
                u32::from_ne_bytes([exe_header[0], exe_header[1], exe_header[2], exe_header[3]]);
            if (magic_new == 0xFEEDFACF || magic_new == 0xFEEDFACE)
                && (magic_exe == 0xFEEDFACF || magic_exe == 0xFEEDFACE)
                && new_header[4..8] != exe_header[4..8]
            {
                return Err("update candidate cpu type does not match".into());
            }
        }
    }

    #[cfg(windows)]
    {
        if &new_header[0..2] != b"MZ" {
            return Err("update candidate is not a PE binary".into());
        }
        if &exe_header[0..2] == b"MZ" {
            let get_pe_machine = |path: &Path| -> Option<[u8; 2]> {
                let mut f = std::fs::File::open(path).ok()?;
                use std::io::{Seek, SeekFrom};
                f.seek(SeekFrom::Start(0x3C)).ok()?;
                let mut offset_bytes = [0u8; 4];
                f.read_exact(&mut offset_bytes).ok()?;
                let pe_offset = u32::from_le_bytes(offset_bytes) as u64;
                f.seek(SeekFrom::Start(pe_offset)).ok()?;
                let mut pe_sig = [0u8; 4];
                f.read_exact(&mut pe_sig).ok()?;
                if &pe_sig != b"PE\0\0" {
                    return None;
                }
                let mut machine = [0u8; 2];
                f.read_exact(&mut machine).ok()?;
                Some(machine)
            };
            if let (Some(m_new), Some(m_exe)) = (get_pe_machine(_new), get_pe_machine(exe))
                && m_new != m_exe
            {
                return Err("update candidate PE machine type does not match".into());
            }
        }
    }

    Ok(())
}

fn binary_contains(path: &Path, needle: &[u8]) -> Result<bool, std::io::Error> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut overlap = Vec::new();
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        let combined = if overlap.is_empty() {
            chunk
        } else {
            overlap.extend_from_slice(chunk);
            &overlap[..]
        };
        if combined.windows(needle.len()).any(|w| w == needle) {
            return Ok(true);
        }
        let keep = needle.len().saturating_sub(1).min(chunk.len());
        overlap.clear();
        overlap.extend_from_slice(&chunk[chunk.len() - keep..]);
    }
    Ok(false)
}

fn check(new: &Path) -> Result<(), String> {
    let mut cmd = Command::new(new);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let out = cmd
        .output()
        .map_err(|e| format!("the new agent does not run here: {e}"))?;
    if out.status.success() && String::from_utf8_lossy(&out.stdout).starts_with("cometdesk-agent ")
    {
        Ok(())
    } else {
        Err("the new agent did not identify itself".into())
    }
}

/// A running program cannot be overwritten on Windows, but it can be renamed.
#[cfg(windows)]
fn swap(exe: &Path, new: &Path) -> Result<(), String> {
    let old = exe.with_extension("old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).map_err(|e| format!("cannot move the running agent: {e}"))?;
    std::fs::rename(new, exe).map_err(|e| {
        let _ = std::fs::rename(&old, exe);
        format!("cannot install the new agent: {e}")
    })
}

#[cfg(unix)]
fn swap(exe: &Path, new: &Path) -> Result<(), String> {
    std::fs::rename(new, exe).map_err(|e| format!("cannot install the new agent: {e}"))
}

/// Removes what an update left behind; called when the agent starts.
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old"));
    }
}

/// Starts `exe` with `args` and ends this process.
pub fn restart(exe: &Path, args: &[String]) -> ! {
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .env(RESTARTED, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
        cmd.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    let _ = cmd.spawn();
    std::process::exit(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_scripts_and_unsigned_binaries() {
        let dir =
            std::env::temp_dir().join(format!("cometdesk-update-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let script = dir.join("test.sh");
        std::fs::write(&script, b"#!/bin/sh\necho cometdesk-agent 0.1.0\n").unwrap();
        let exe = std::env::current_exe().unwrap();

        // 1. Too small file / script with shebang should be rejected
        let err = verify_candidate(&script, &exe).unwrap_err();
        assert!(err.contains("script") || err.contains("size"));

        // 2. Large file starting with shebang
        let mut big_script = vec![0u8; 100 * 1024];
        big_script[..9].copy_from_slice(b"#!/bin/sh");
        std::fs::write(&script, &big_script).unwrap();
        let err = verify_candidate(&script, &exe).unwrap_err();
        assert!(err.contains("script"));

        // 3. Fake binary without valid header
        let fake_bin = dir.join("fake.bin");
        let fake_content = vec![0u8; 100 * 1024];
        std::fs::write(&fake_bin, &fake_content).unwrap();
        let err = verify_candidate(&fake_bin, &exe).unwrap_err();
        assert!(
            err.contains("not an ELF") || err.contains("not a Mach-O") || err.contains("not a PE")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
