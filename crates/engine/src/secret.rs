//! Secrets encrypted for the current Windows user with DPAPI.

use crate::error::EngineError;
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};

fn blob_in(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
}

/// Copies DPAPI's output and frees it.
fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    // SAFETY: DPAPI returned `cbData` bytes at `pbData`, allocated with LocalAlloc.
    let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe { LocalFree(out.pbData as _) };
    v
}

fn failed(what: &str) -> EngineError {
    EngineError::Io(std::io::Error::other(format!("{what}: {}", std::io::Error::last_os_error())))
}

/// Encrypts `plain` so only the current Windows user can read it.
pub fn protect(plain: &[u8]) -> Result<Vec<u8>, EngineError> {
    let input = blob_in(plain);
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // SAFETY: every pointer is valid for the call; optional ones are null.
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    };
    if ok == 0 {
        return Err(failed("could not encrypt"));
    }
    Ok(take(out))
}

/// Decrypts a blob made by `protect` for this user.
pub fn unprotect(blob: &[u8]) -> Result<Vec<u8>, EngineError> {
    let input = blob_in(blob);
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // SAFETY: as in `protect`.
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    };
    if ok == 0 {
        return Err(failed("could not decrypt"));
    }
    Ok(take(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpapi_roundtrip() {
        let blob = protect(b"pw").unwrap();
        assert_ne!(blob, b"pw".to_vec());
        assert_eq!(unprotect(&blob).unwrap(), b"pw".to_vec());
    }

    #[test]
    fn unprotect_garbage_errs() {
        assert!(unprotect(b"not a dpapi blob").is_err());
    }
}
