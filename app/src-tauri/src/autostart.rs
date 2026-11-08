//! Start with Windows: Haul's own `HKCU\...\Run` entry, with the exe path in quotes.
//!
//! `tauri-plugin-autostart` wrote `path args` unquoted, so with spaces in the path Windows first
//! tried `C:\tools\claude.exe` and the like at sign-in. This module writes `"path" --hidden`.

use std::io;
use std::path::Path;
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};

/// The Run value's name.
pub const NAME: &str = "Haul";
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

/// The Run value for `exe`: the path in quotes, then `--hidden`.
pub fn run_value(exe: &Path) -> String {
    let shown = exe.to_string_lossy();
    format!("\"{}\" --hidden", shown.trim_start_matches(r"\\?\"))
}

/// Haul's Start with Windows entry under `HKEY_CURRENT_USER`.
pub struct Autostart {
    run: String,
    approved: String,
}

impl Autostart {
    /// The real `Run` key.
    pub fn real() -> Autostart {
        Autostart { run: RUN.into(), approved: APPROVED.into() }
    }

    /// A copy of the layout under `root` (`<root>\Run`, `<root>\StartupApproved\Run`), for tests.
    pub fn at(root: &str) -> Autostart {
        Autostart { run: format!(r"{root}\Run"), approved: format!(r"{root}\StartupApproved\Run") }
    }

    /// Writes the quoted value, and clears a "disabled" mark Task Manager left, so the switch works.
    pub fn enable(&self, exe: &Path) -> io::Result<()> {
        self.write_run(exe)?;
        if read_binary(&self.approved, NAME)?.is_some_and(|b| b.first().is_some_and(|f| f % 2 == 1)) {
            delete_value(&self.approved, NAME)?;
        }
        Ok(())
    }

    /// Removes the value; a missing one is fine.
    pub fn disable(&self) -> io::Result<()> {
        delete_value(&self.run, NAME)
    }

    pub fn is_enabled(&self) -> io::Result<bool> {
        Ok(self.value()?.is_some())
    }

    /// The Run value as stored, if any.
    pub fn value(&self) -> io::Result<Option<String>> {
        read_string(&self.run, NAME)
    }

    fn write_run(&self, exe: &Path) -> io::Result<()> {
        write_string(&self.run, NAME, &run_value(exe))
    }
}

/// At start: with the setting on, rewrites the value quoted for the current exe (repairing an old
/// unquoted one, following a moved exe) but leaves Task Manager's mark alone. A value removed outside
/// Haul turns the setting off. Returns the setting's new value.
pub fn repair(a: &Autostart, setting_on: bool, exe: &Path) -> io::Result<bool> {
    if !setting_on {
        return Ok(false);
    }
    match a.value()? {
        None => Ok(false),
        Some(v) => {
            if v != run_value(exe) {
                a.write_run(exe)?;
            }
            Ok(true)
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn check(code: WIN32_ERROR) -> io::Result<()> {
    if code == ERROR_SUCCESS { Ok(()) } else { Err(io::Error::from_raw_os_error(code as i32)) }
}

fn write_raw(key: &str, name: &str, kind: u32, data: &[u8]) -> io::Result<()> {
    let (k, n) = (wide(key), wide(name));
    // SAFETY: both strings are NUL-terminated and live across the call; data's length is passed.
    check(unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr(), kind, data.as_ptr().cast(), data.len() as u32)
    })
}

pub(crate) fn write_string(key: &str, name: &str, value: &str) -> io::Result<()> {
    let bytes: Vec<u8> = wide(value).iter().flat_map(|u| u.to_le_bytes()).collect();
    write_raw(key, name, REG_SZ, &bytes)
}

#[cfg(test)]
pub(crate) fn write_binary(key: &str, name: &str, value: &[u8]) {
    write_raw(key, name, windows_sys::Win32::System::Registry::REG_BINARY, value).unwrap();
}

/// Reads a value's bytes with `RegGetValueW`; a missing key or value is `None`.
fn read_raw(key: &str, name: &str, flags: u32) -> io::Result<Option<Vec<u8>>> {
    let (k, n) = (wide(key), wide(name));
    let mut size = 0u32;
    // SAFETY: a size query (null buffer) on NUL-terminated strings.
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            k.as_ptr(),
            n.as_ptr(),
            flags,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    check(code)?;
    let mut buf = vec![0u8; size as usize];
    // SAFETY: buf holds `size` bytes, as the call is told.
    check(unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            k.as_ptr(),
            n.as_ptr(),
            flags,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    })?;
    buf.truncate(size as usize);
    Ok(Some(buf))
}

fn read_string(key: &str, name: &str) -> io::Result<Option<String>> {
    Ok(read_raw(key, name, RRF_RT_REG_SZ)?.map(|b| {
        let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        String::from_utf16_lossy(&units).trim_end_matches('\0').to_string()
    }))
}

fn read_binary(key: &str, name: &str) -> io::Result<Option<Vec<u8>>> {
    read_raw(key, name, RRF_RT_REG_BINARY)
}

fn delete_value(key: &str, name: &str) -> io::Result<()> {
    let (k, n) = (wide(key), wide(name));
    // SAFETY: NUL-terminated strings.
    let code = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr()) };
    if code == ERROR_FILE_NOT_FOUND { Ok(()) } else { check(code) }
}

#[cfg(test)]
pub(crate) fn has_value(key: &str, name: &str) -> bool {
    read_raw(key, name, windows_sys::Win32::System::Registry::RRF_RT_ANY).unwrap().is_some()
}

#[cfg(test)]
pub(crate) fn delete_tree(key: &str) {
    let k = wide(key);
    // SAFETY: a NUL-terminated scratch key under HKCU\Software\Haul-test-*.
    unsafe { windows_sys::Win32::System::Registry::RegDeleteTreeW(HKEY_CURRENT_USER, k.as_ptr()) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// A scratch key under `HKCU\Software`, deleted when dropped.
    struct Scratch {
        root: String,
        a: Autostart,
    }

    impl Scratch {
        fn new() -> Scratch {
            let mut b = [0u8; 8];
            getrandom::fill(&mut b).unwrap();
            let root = format!(r"Software\Haul-test-{}", u64::from_le_bytes(b));
            let a = Autostart::at(&root);
            Scratch { root, a }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            delete_tree(&self.root);
        }
    }

    fn exe() -> PathBuf {
        PathBuf::from(r"C:\Users\Jane Doe\AppData\Local\Haul\haul.exe")
    }

    #[test]
    fn run_value_quotes_spaces() {
        assert_eq!(
            run_value(Path::new(r"C:\Users\Jane Doe\AppData\Local\Haul\haul.exe")),
            r#""C:\Users\Jane Doe\AppData\Local\Haul\haul.exe" --hidden"#
        );
    }

    #[test]
    fn run_value_strips_verbatim_prefix() {
        assert_eq!(run_value(Path::new(r"\\?\C:\a b\haul.exe")), r#""C:\a b\haul.exe" --hidden"#);
    }

    #[test]
    fn enable_writes_quoted_value() {
        let s = Scratch::new();
        s.a.enable(&exe()).unwrap();
        assert_eq!(s.a.value().unwrap().as_deref(), Some(run_value(&exe()).as_str()));
    }

    #[test]
    fn disable_removes_value() {
        let s = Scratch::new();
        s.a.enable(&exe()).unwrap();
        s.a.disable().unwrap();
        assert_eq!(s.a.value().unwrap(), None);
    }

    #[test]
    fn disable_twice_is_fine() {
        let s = Scratch::new();
        s.a.disable().unwrap();
        s.a.disable().unwrap();
    }

    #[test]
    fn is_enabled_follows_value() {
        let s = Scratch::new();
        assert!(!s.a.is_enabled().unwrap());
        s.a.enable(&exe()).unwrap();
        assert!(s.a.is_enabled().unwrap());
        s.a.disable().unwrap();
        assert!(!s.a.is_enabled().unwrap());
    }

    #[test]
    fn enable_clears_startup_approved_disabled() {
        let s = Scratch::new();
        let approved = format!(r"{}\StartupApproved\Run", s.root);
        write_binary(&approved, NAME, &[3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        s.a.enable(&exe()).unwrap();
        assert!(!has_value(&approved, NAME));
    }

    #[test]
    fn enable_keeps_startup_approved_enabled_mark() {
        let s = Scratch::new();
        let approved = format!(r"{}\StartupApproved\Run", s.root);
        write_binary(&approved, NAME, &[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        s.a.enable(&exe()).unwrap();
        assert!(has_value(&approved, NAME));
    }

    #[test]
    fn repair_rewrites_unquoted() {
        let s = Scratch::new();
        write_string(&format!(r"{}\Run", s.root), NAME, r"C:\a b\haul.exe --hidden").unwrap();
        assert!(repair(&s.a, true, &exe()).unwrap());
        assert_eq!(s.a.value().unwrap(), Some(run_value(&exe())));
    }

    #[test]
    fn repair_turns_setting_off_when_value_gone() {
        let s = Scratch::new();
        assert!(!repair(&s.a, true, &exe()).unwrap());
        assert_eq!(s.a.value().unwrap(), None);
    }

    #[test]
    fn repair_leaves_off_alone() {
        let s = Scratch::new();
        assert!(!repair(&s.a, false, &exe()).unwrap());
        assert_eq!(s.a.value().unwrap(), None);
    }
}
