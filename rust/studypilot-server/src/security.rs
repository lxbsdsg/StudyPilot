use anyhow::Context;
#[cfg(not(windows))]
use std::path::Path;
#[cfg(windows)]
use std::path::Path;
#[cfg(not(windows))]
use {
    aes_gcm::{
        Aes256Gcm, Nonce,
        aead::{Aead, KeyInit},
    },
    std::{fs, io::Write},
};

pub fn protect_password(password: &str, secret_file: &Path) -> anyhow::Result<Vec<u8>> {
    if password.is_empty() {
        return Ok(Vec::new());
    }
    #[cfg(windows)]
    {
        let _ = secret_file;
        dpapi::protect(password.as_bytes())
    }
    #[cfg(not(windows))]
    {
        let key = load_or_create_key(secret_file)?;
        let cipher = Aes256Gcm::new_from_slice(&key)?;
        let mut nonce_bytes = [0_u8; 12];
        getrandom::fill(&mut nonce_bytes)
            .map_err(|error| anyhow::anyhow!("无法生成随机数：{error}"))?;
        let encrypted = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), password.as_bytes())
            .map_err(|_| anyhow::anyhow!("无法加密账号密码"))?;
        let mut protected = Vec::with_capacity(12 + encrypted.len());
        protected.extend_from_slice(&nonce_bytes);
        protected.extend_from_slice(&encrypted);
        Ok(protected)
    }
}

pub fn unprotect_password(protected: &[u8], secret_file: &Path) -> anyhow::Result<String> {
    if protected.is_empty() {
        return Ok(String::new());
    }
    #[cfg(windows)]
    {
        let _ = secret_file;
        let bytes = dpapi::unprotect(protected)?;
        String::from_utf8(bytes).context("解密后的密码不是 UTF-8")
    }
    #[cfg(not(windows))]
    {
        anyhow::ensure!(protected.len() >= 12, "加密密码长度无效");
        let key = load_or_create_key(secret_file)?;
        let cipher = Aes256Gcm::new_from_slice(&key)?;
        let bytes = cipher
            .decrypt(Nonce::from_slice(&protected[..12]), &protected[12..])
            .map_err(|_| anyhow::anyhow!("无法解密账号密码"))?;
        return String::from_utf8(bytes).context("解密后的密码不是 UTF-8");
    }
}

#[cfg(not(windows))]
fn load_or_create_key(path: &Path) -> anyhow::Result<Vec<u8>> {
    if path.exists() {
        let key = fs::read(path)?;
        anyhow::ensure!(key.len() == 32, "凭据密钥长度无效");
        return Ok(key);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut key = [0_u8; 32];
    getrandom::fill(&mut key).map_err(|error| anyhow::anyhow!("无法生成凭据密钥：{error}"))?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(&key)?;
            Ok(key.to_vec())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => load_or_create_key(path),
        Err(error) => Err(error.into()),
    }
}

#[cfg(windows)]
mod dpapi {
    use std::ffi::c_void;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[link(name = "Crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            data_in: *const DataBlob,
            data_description: *const u16,
            optional_entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt_struct: *mut c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
        fn CryptUnprotectData(
            data_in: *const DataBlob,
            data_description: *mut *mut u16,
            optional_entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt_struct: *mut c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
    }

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn LocalFree(handle: *mut c_void) -> *mut c_void;
    }

    fn make_blob(data: &mut [u8]) -> DataBlob {
        DataBlob {
            cb_data: data.len() as u32,
            pb_data: data.as_mut_ptr(),
        }
    }

    fn copy_output(blob: &DataBlob) -> Vec<u8> {
        if blob.pb_data.is_null() || blob.cb_data == 0 {
            return Vec::new();
        }
        unsafe {
            let result =
                std::slice::from_raw_parts(blob.pb_data.cast_const(), blob.cb_data as usize)
                    .to_vec();
            LocalFree(blob.pb_data.cast());
            result
        }
    }

    pub fn protect(data: &[u8]) -> anyhow::Result<Vec<u8>> {
        let mut input = data.to_vec();
        let input_blob = make_blob(&mut input);
        let mut output_blob = DataBlob {
            cb_data: 0,
            pb_data: std::ptr::null_mut(),
        };
        let ok = unsafe {
            CryptProtectData(
                &input_blob,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0x1,
                &mut output_blob,
            )
        };
        anyhow::ensure!(
            ok != 0,
            "Windows DPAPI 加密失败：{}",
            std::io::Error::last_os_error()
        );
        Ok(copy_output(&output_blob))
    }

    pub fn unprotect(data: &[u8]) -> anyhow::Result<Vec<u8>> {
        let mut input = data.to_vec();
        let input_blob = make_blob(&mut input);
        let mut output_blob = DataBlob {
            cb_data: 0,
            pb_data: std::ptr::null_mut(),
        };
        let ok = unsafe {
            CryptUnprotectData(
                &input_blob,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0x1,
                &mut output_blob,
            )
        };
        anyhow::ensure!(
            ok != 0,
            "Windows DPAPI 解密失败：{}",
            std::io::Error::last_os_error()
        );
        Ok(copy_output(&output_blob))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_round_trip_uses_platform_protection() {
        let directory = tempfile::tempdir().unwrap();
        let key_file = directory.path().join("secret.key");
        let protected = protect_password("test-password", &key_file).unwrap();
        assert!(!protected.is_empty());
        assert_ne!(protected, b"test-password");
        assert_eq!(
            unprotect_password(&protected, &key_file).unwrap(),
            "test-password"
        );
    }
}
