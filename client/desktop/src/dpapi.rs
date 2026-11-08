//! Windows DPAPI (CryptProtectData) — protects the persisted session token,
//! offline-login cache and (once SQLCipher lands) the SQLCipher key so the
//! plaintext never sits on disk. Non-Windows builds fall through (the first
//! desktop version is Windows-only per README §11).

#[cfg(windows)]
pub fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
    use winapi::um::dpapi::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN};
    use winapi::um::wincrypt::CRYPT_INTEGER_BLOB;

    unsafe {
        let mut in_blob = CRYPT_INTEGER_BLOB {
            cbData: plain.len() as u32,
            pbData: plain.as_ptr() as *mut u8,
        };
        let mut out_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let ok = CryptProtectData(
            &mut in_blob,
            std::ptr::null(),     // description
            std::ptr::null_mut(), // entropy (none: per-user_MACHINE scope implicit)
            std::ptr::null_mut(),
            std::ptr::null_mut(), // prompt struct
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        );
        if ok == 0 {
            return Err("CryptProtectData failed".into());
        }
        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize);
        let out = slice.to_vec();
        winapi::um::winbase::LocalFree(out_blob.pbData as _);
        Ok(out)
    }
}

#[cfg(windows)]
pub fn unprotect(protected: &[u8]) -> Result<Vec<u8>, String> {
    use winapi::um::dpapi::{CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN};
    use winapi::um::wincrypt::CRYPT_INTEGER_BLOB;

    unsafe {
        let mut in_blob = CRYPT_INTEGER_BLOB {
            cbData: protected.len() as u32,
            pbData: protected.as_ptr() as *mut u8,
        };
        let mut out_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let ok = CryptUnprotectData(
            &mut in_blob,
            std::ptr::null_mut(), // description out
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(), // prompt struct
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        );
        if ok == 0 {
            return Err("CryptUnprotectData failed".into());
        }
        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize);
        let out = slice.to_vec();
        winapi::um::winbase::LocalFree(out_blob.pbData as _);
        Ok(out)
    }
}

#[cfg(not(windows))]
pub fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
    Ok(plain.to_vec())
}

#[cfg(not(windows))]
pub fn unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
    Ok(blob.to_vec())
}

/// 32 random bytes for a SQLCipher key (or any local secret): the uuid crate's
/// v4 generator is backed by an OS-seeded RNG, and each v4 carries 122 random
/// bits — four of them comfortably exceed a 256-bit key's entropy budget.
/// Reserved for the SQLCipher integration (deferred; see todo Phase 6).
#[allow(dead_code)]
pub fn random_bytes(n: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(n + 16);
    while out.len() < n {
        out.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    }
    out.truncate(n);
    Ok(out)
}
