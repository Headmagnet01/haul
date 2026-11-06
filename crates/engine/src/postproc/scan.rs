//! The optional Windows Defender scan of finished files.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanOutcome {
    Clean,
    Threat,
    /// Defender is missing or couldn't scan.
    Unavailable,
}

/// Scans a file or a whole folder. Blocking.
pub trait Scanner: Send + Sync {
    fn scan(&self, path: &Path) -> ScanOutcome;
}

/// `MpCmdRun.exe`, Windows Defender's command-line scanner.
pub struct Defender {
    exe: PathBuf,
}

impl Default for Defender {
    fn default() -> Self {
        Defender { exe: Self::default_path() }
    }
}

impl Defender {
    pub fn default_path() -> PathBuf {
        let pf = std::env::var_os("ProgramFiles").map_or_else(|| PathBuf::from(r"C:\Program Files"), PathBuf::from);
        pf.join(r"Windows Defender\MpCmdRun.exe")
    }

    /// A scanner at another path (tests).
    pub fn at(exe: PathBuf) -> Self {
        Defender { exe }
    }
}

impl Scanner for Defender {
    fn scan(&self, path: &Path) -> ScanOutcome {
        use std::os::windows::process::CommandExt;
        if !self.exe.is_file() {
            return ScanOutcome::Unavailable;
        }
        let status = Command::new(&self.exe)
            .args(["-Scan", "-ScanType", "3", "-File"])
            .arg(path)
            .arg("-DisableRemediation")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .status();
        match status.map(|s| s.code()) {
            Ok(Some(0)) => ScanOutcome::Clean,
            Ok(Some(2)) => ScanOutcome::Threat,
            _ => ScanOutcome::Unavailable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defender_missing_is_unavailable() {
        let t = tempfile::tempdir().unwrap();
        let d = Defender::at(t.path().join("MpCmdRun.exe"));
        assert_eq!(d.scan(t.path()), ScanOutcome::Unavailable);
    }

    #[test]
    fn defender_default_path() {
        let p = Defender::default_path();
        assert!(p.ends_with(r"Windows Defender\MpCmdRun.exe"), "{}", p.display());
    }
}
