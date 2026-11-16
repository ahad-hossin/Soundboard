//! Add a sound from a link: finds yt-dlp and ffmpeg (installed by winget) and runs yt-dlp.

use std::ffi::OsString;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Longest video accepted: sounds are fully decoded into memory.
pub const MAX_LINK_SECS: u32 = 600;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(180);
const UPGRADE_TIMEOUT: Duration = Duration::from_secs(120);

pub fn is_link(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

pub fn find_in_dirs(dirs: &[PathBuf], exe: &str) -> Option<PathBuf> {
    dirs.iter().filter(|d| d.is_absolute()).map(|d| d.join(exe)).find(|p| p.is_file())
}

/// Process PATH, then user and machine PATH read fresh from the registry (winget edits them
/// after this process started), then winget's Links folder.
pub fn find_tool(exe: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    for p in registry_paths() {
        dirs.extend(std::env::split_paths(&p));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join(r"Microsoft\WinGet\Links"));
    }
    find_in_dirs(&dirs, exe)
}

#[cfg(windows)]
fn registry_paths() -> Vec<OsString> {
    use std::os::windows::ffi::OsStringExt;
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let read = |key, sub| -> Option<OsString> {
        let mut len = 0u32;
        // RRF_RT_REG_SZ alone also accepts REG_EXPAND_SZ and expands %VARS% for us.
        unsafe { RegGetValueW(key, sub, w!("Path"), RRF_RT_REG_SZ, None, None, Some(&mut len)) }.ok().ok()?;
        let mut buf = vec![0u16; (len as usize).div_ceil(2)];
        unsafe { RegGetValueW(key, sub, w!("Path"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut len)) }.ok().ok()?;
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(OsString::from_wide(&buf[..end]))
    };
    [
        read(HKEY_CURRENT_USER, w!("Environment")),
        read(HKEY_LOCAL_MACHINE, w!(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(not(windows))]
fn registry_paths() -> Vec<OsString> {
    Vec::new()
}

pub fn ytdlp_args(url: &str, ffmpeg: &Path, out_dir: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "--ignore-config", "--no-playlist", "--playlist-items", "1", "--no-progress", "-x", "--audio-format", "mp3",
        "--match-filter", &format!("duration<=?{MAX_LINK_SECS}"), "--ffmpeg-location",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.push(ffmpeg.into());
    args.push("-o".into());
    args.push(out_dir.join("%(title).80B.%(ext)s").into());
    args.push(url.into());
    args
}

pub fn error_line(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .find_map(|l| l.strip_prefix("ERROR: "))
        .map(|l| {
            let mut l = l.trim();
            if l.starts_with('[') {
                l = l.split_once("] ").map_or(l, |(_, rest)| rest);
            }
            // yt-dlp's "<video id>: " prefix.
            match l.split_once(' ') {
                Some((tok, rest)) if tok.ends_with(':') => rest.to_string(),
                _ => l.to_string(),
            }
        })
        .unwrap_or_else(|| "Download failed".to_string())
}

fn hidden(exe: &Path) -> Command {
    let mut cmd = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Runs to completion or kills it after `timeout`; stderr goes to `stderr_file`. Ok(true) on exit code 0.
fn run(mut cmd: Command, stderr_file: &Path, timeout: Duration) -> Result<bool, String> {
    let err = File::create(stderr_file).map_err(|e| e.to_string())?;
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(err).spawn().map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Ok(status.success());
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err("The download took too long and was stopped".to_string());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn first_mp3(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp3")))
}

fn is_winget(p: &Path) -> bool {
    p.to_string_lossy().to_ascii_lowercase().contains(r"\microsoft\winget\")
}

/// Downloads `url` as an mp3 into `out_dir` (which must exist). A failure with a winget-installed
/// yt-dlp upgrades it once and retries, since an outdated yt-dlp is the usual cause.
pub fn download_audio(url: &str, out_dir: &Path) -> Result<PathBuf, String> {
    let ytdlp = find_tool("yt-dlp.exe").ok_or("yt-dlp is not installed. Install it with: winget install -e --id yt-dlp.yt-dlp")?;
    let ffmpeg = find_tool("ffmpeg.exe").ok_or("ffmpeg is not installed. Install it with: winget install -e --id Gyan.FFmpeg")?;
    let stderr = out_dir.join("yt-dlp.log");
    let attempt = || -> Result<Option<PathBuf>, String> {
        let mut cmd = hidden(&ytdlp);
        cmd.args(ytdlp_args(url, &ffmpeg, out_dir));
        if !run(cmd, &stderr, DOWNLOAD_TIMEOUT)? {
            return Ok(None);
        }
        // Exit 0 without a file: the match filter skipped a video longer than the cap.
        first_mp3(out_dir).map(Some).ok_or_else(|| format!("Videos longer than {} minutes are not supported", MAX_LINK_SECS / 60))
    };
    if let Some(mp3) = attempt()? {
        return Ok(mp3);
    }
    if is_winget(&ytdlp) {
        if let Some(winget) = find_tool("winget.exe") {
            let mut cmd = hidden(&winget);
            cmd.args(["upgrade", "-e", "--id", "yt-dlp.yt-dlp", "--accept-source-agreements", "--accept-package-agreements"]);
            let _ = run(cmd, &out_dir.join("winget.log"), UPGRADE_TIMEOUT);
            if let Some(mp3) = attempt()? {
                return Ok(mp3);
            }
        }
    }
    Err(error_line(&fs::read_to_string(&stderr).unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn only_http_links_count() {
        assert!(is_link("https://youtu.be/abc"));
        assert!(is_link("http://example.com/a"));
        assert!(!is_link("ftp://x"));
        assert!(!is_link("C:\\a.mp3"));
        assert!(!is_link(""));
    }

    #[test]
    fn finds_the_first_dir_holding_the_exe() {
        let (a, b) = (tempdir().unwrap(), tempdir().unwrap());
        std::fs::write(b.path().join("yt-dlp.exe"), b"").unwrap();
        let dirs = vec![a.path().to_path_buf(), b.path().to_path_buf()];
        assert_eq!(find_in_dirs(&dirs, "yt-dlp.exe"), Some(b.path().join("yt-dlp.exe")));
        assert_eq!(find_in_dirs(&dirs, "ffmpeg.exe"), None);
    }

    #[test]
    fn skips_relative_and_empty_dirs() {
        // cargo tests run with cwd = the crate dir, which holds Cargo.toml.
        assert_eq!(find_in_dirs(&[PathBuf::from("")], "Cargo.toml"), None);
        assert_eq!(find_in_dirs(&[PathBuf::from("rel")], "Cargo.toml"), None);
        let a = tempdir().unwrap();
        std::fs::write(a.path().join("x.exe"), b"").unwrap();
        let dirs = vec![PathBuf::from(""), a.path().to_path_buf()];
        assert_eq!(find_in_dirs(&dirs, "x.exe"), Some(a.path().join("x.exe")));
    }

    #[test]
    fn args_extract_mp3_with_cap_and_no_playlist() {
        let args: Vec<String> = ytdlp_args("https://x.test/v", Path::new("C:/ff/ffmpeg.exe"), Path::new("C:/tmp/o"))
            .into_iter().map(|a| a.to_string_lossy().into_owned()).collect();
        for want in ["--no-playlist", "-x", "--ignore-config", "--no-progress"] {
            assert!(args.contains(&want.to_string()), "{want}");
        }
        let after = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
        assert_eq!(after("--audio-format"), "mp3");
        assert_eq!(after("--playlist-items"), "1");
        assert_eq!(after("--match-filter"), "duration<=?600");
        assert_eq!(after("--ffmpeg-location"), "C:/ff/ffmpeg.exe");
        assert!(after("-o").starts_with("C:/tmp/o"));
        assert_eq!(args.last().unwrap(), "https://x.test/v");
    }

    #[test]
    fn error_line_is_the_last_yt_dlp_error() {
        let stderr = "WARNING: x\nERROR: [youtube] abc: Video unavailable\nERROR: [instagram] Requested content is not available, rate-limit reached or login required\n";
        assert_eq!(error_line(stderr), "Requested content is not available, rate-limit reached or login required");
        assert_eq!(error_line("ERROR: [youtube] abc123: This video is unavailable"), "This video is unavailable");
        assert_eq!(error_line("garbage"), "Download failed");
    }
}
