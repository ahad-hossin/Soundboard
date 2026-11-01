//! Windows Firewall status for the phone remote, and a one-click elevated "allow" rule.
//!
//! Both operations shell out to PowerShell: reading rules needs no elevation, adding one goes
//! through a UAC prompt. Nothing here touches the `App` mutex.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde::Serialize;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
/// Name of the rule `allow` adds.
const RULE_NAME: &str = "Soundboard";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FirewallState {
    Allowed,
    Blocked,
    None,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct FirewallStatus {
    pub exe: String,
    pub state: FirewallState,
}

/// Path of the running executable as the firewall sees it: canonical, without the `\\?\` prefix.
pub fn exe_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    Some(PathBuf::from(strip_verbatim(&exe.to_string_lossy())))
}

fn strip_verbatim(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        path.to_string()
    }
}

/// Turns the `Action` lines printed by the query into a state. A block rule beats an allow rule.
pub fn parse_actions(output: &str) -> FirewallState {
    let mut allowed = false;
    for line in output.lines().map(str::trim) {
        if line.eq_ignore_ascii_case("block") {
            return FirewallState::Blocked;
        }
        if line.eq_ignore_ascii_case("allow") {
            allowed = true;
        }
    }
    if allowed {
        FirewallState::Allowed
    } else {
        FirewallState::None
    }
}

/// Single-quoted PowerShell string literal.
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn powershell() -> Command {
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn query_script(exe: &Path) -> String {
    format!(
        "Get-NetFirewallApplicationFilter | Where-Object {{ $_.Program -ieq {} }} | Get-NetFirewallRule | \
         Where-Object {{ $_.Direction -eq 'Inbound' -and $_.Enabled -eq 'True' }} | ForEach-Object {{ $_.Action }}",
        ps_quote(&exe.to_string_lossy())
    )
}

/// Reads the inbound rules that name this program. Any failure, or no answer within ten
/// seconds, is `Unknown`.
pub fn check(exe: &Path) -> FirewallState {
    let mut child = match powershell()
        .args(["-Command", &query_script(exe)])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log::warn!("firewall check could not start PowerShell: {e}");
            return FirewallState::Unknown;
        }
    };
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        return FirewallState::Unknown;
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    let Ok(buf) = rx.recv_timeout(CHECK_TIMEOUT) else {
        log::warn!("firewall check timed out");
        // Killing the process closes its pipe, which ends the reader thread.
        let _ = child.kill();
        let _ = child.wait();
        return FirewallState::Unknown;
    };
    match child.wait() {
        Ok(status) if status.success() => parse_actions(&String::from_utf8_lossy(&buf)),
        _ => FirewallState::Unknown,
    }
}

/// Status of the running executable; `Unknown` when its path cannot be determined.
pub fn status() -> FirewallStatus {
    match exe_path() {
        Some(exe) => FirewallStatus { state: check(&exe), exe: exe.to_string_lossy().into_owned() },
        None => FirewallStatus { exe: String::new(), state: FirewallState::Unknown },
    }
}

/// Script the elevated PowerShell runs: drop every inbound rule for this program (Windows
/// creates Block rules named after the exe when its prompt is dismissed, and a block beats an
/// allow), then add one allow rule for all profiles. Exits with netsh's code for the add.
fn elevated_script(exe: &Path) -> String {
    format!(
        "$exe = {}; \
         netsh advfirewall firewall delete rule name=all dir=in \"program=$exe\" | Out-Null; \
         netsh advfirewall firewall add rule \"name={RULE_NAME}\" dir=in action=allow \"program=$exe\" enable=yes profile=any | Out-Null; \
         exit $LASTEXITCODE",
        ps_quote(&exe.to_string_lossy())
    )
}

/// Script run non-elevated: starts the elevated one through UAC and waits for it.
fn launcher_script(exe: &Path) -> String {
    let encoded = encode_command(&elevated_script(exe));
    format!(
        "try {{ $p = Start-Process powershell -Verb RunAs -Wait -PassThru -WindowStyle Hidden \
         -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand','{encoded}'; exit $p.ExitCode }} \
         catch {{ exit 1 }}"
    )
}

/// Adds an inbound allow rule for `exe` through a UAC prompt.
pub fn allow(exe: &Path) -> Result<(), String> {
    let status = powershell()
        .args(["-Command", &launcher_script(exe)])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("could not start PowerShell: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("cancelled or failed".to_string())
    }
}

/// `-EncodedCommand` payload: base64 of the UTF-16LE text, so no quoting survives to bite.
fn encode_command(script: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_rule_wins_over_an_allow_rule() {
        assert_eq!(parse_actions("Block\nAllow"), FirewallState::Blocked);
        assert_eq!(parse_actions("Allow\r\nBlock\r\n"), FirewallState::Blocked);
    }

    #[test]
    fn allow_only_is_allowed() {
        assert_eq!(parse_actions("Allow"), FirewallState::Allowed);
        assert_eq!(parse_actions("Allow\nAllow\n"), FirewallState::Allowed);
    }

    #[test]
    fn no_output_means_no_rule() {
        assert_eq!(parse_actions(""), FirewallState::None);
        assert_eq!(parse_actions("\r\n  \n"), FirewallState::None);
    }

    #[test]
    fn serializes_lowercase() {
        assert_eq!(serde_json::to_string(&FirewallState::Blocked).unwrap(), "\"blocked\"");
        assert_eq!(serde_json::to_string(&FirewallState::None).unwrap(), "\"none\"");
    }

    #[test]
    fn strips_the_verbatim_prefix() {
        assert_eq!(strip_verbatim(r"\\?\C:\Apps\a.exe"), r"C:\Apps\a.exe");
        assert_eq!(strip_verbatim(r"\\?\UNC\srv\share\a.exe"), r"\\srv\share\a.exe");
        assert_eq!(strip_verbatim(r"C:\Apps\a.exe"), r"C:\Apps\a.exe");
    }

    #[test]
    fn quotes_single_quotes_in_the_path() {
        assert_eq!(ps_quote(r"C:\Bob's apps\a.exe"), r"'C:\Bob''s apps\a.exe'");
    }

    #[test]
    fn base64_of_utf16le() {
        // "A" is 41 00, "AB" is 41 00 42 00, "ABC" is 41 00 42 00 43 00.
        assert_eq!(encode_command("A"), "QQA=");
        assert_eq!(encode_command("AB"), "QQBCAA==");
        assert_eq!(encode_command("ABC"), "QQBCAEMA");
    }

    #[test]
    #[ignore = "runs PowerShell against the real firewall"]
    fn check_reads_this_machine() {
        let exe = exe_path().expect("exe path");
        let state = check(&exe);
        println!("{state:?} for {}", exe.display());
        assert_ne!(state, FirewallState::Unknown);
    }

    #[test]
    fn scripts_embed_the_quoted_path() {
        let exe = Path::new(r"C:\Program Files\Soundboard\soundboard.exe");
        assert!(query_script(exe).contains(r"-ieq 'C:\Program Files\Soundboard\soundboard.exe'"));
        assert!(elevated_script(exe).contains(r"$exe = 'C:\Program Files\Soundboard\soundboard.exe';"));
    }
}
