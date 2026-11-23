//! AES-GCM transport using the Windows cryptography provider.

use anyhow::{Result, anyhow, ensure};
use windows::Win32::Security::Cryptography::{
    BCRYPT_AES_ALGORITHM, BCRYPT_ALG_HANDLE, BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO,
    BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION, BCRYPT_CHAINING_MODE, BCRYPT_FLAGS,
    BCRYPT_HANDLE, BCRYPT_KEY_HANDLE, BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS,
    BCryptCloseAlgorithmProvider, BCryptDecrypt, BCryptDestroyKey, BCryptEncrypt,
    BCryptGenerateSymmetricKey, BCryptOpenAlgorithmProvider, BCryptSetProperty,
};

struct Algorithm(BCRYPT_ALG_HANDLE);
struct Key(BCRYPT_KEY_HANDLE);

impl Drop for Algorithm {
    fn drop(&mut self) {
        unsafe {
            let _ = BCryptCloseAlgorithmProvider(self.0, 0);
        }
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = BCryptDestroyKey(self.0);
        }
    }
}

pub(super) fn crypt(
    key: &[u8],
    nonce: &[u8; 12],
    aad: &[u8],
    data: &[u8],
    encrypt: bool,
) -> Result<Vec<u8>> {
    ensure!(key.len() == 32, "invalid browser session key");
    let mut algorithm = BCRYPT_ALG_HANDLE::default();
    unsafe {
        BCryptOpenAlgorithmProvider(
            &mut algorithm,
            BCRYPT_AES_ALGORITHM,
            None,
            BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0),
        )
        .ok()
        .map_err(|_| anyhow!("cannot open browser session cipher"))?;
    }
    let algorithm = Algorithm(algorithm);
    let mode: Vec<u8> = "ChainingModeGCM\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    unsafe {
        BCryptSetProperty(BCRYPT_HANDLE(algorithm.0.0), BCRYPT_CHAINING_MODE, &mode, 0)
            .ok()
            .map_err(|_| anyhow!("cannot configure browser session cipher"))?;
    }
    let mut key_handle = BCRYPT_KEY_HANDLE::default();
    unsafe {
        BCryptGenerateSymmetricKey(algorithm.0, &mut key_handle, None, key, 0)
            .ok()
            .map_err(|_| anyhow!("cannot initialize browser session key"))?;
    }
    let key_handle = Key(key_handle);
    let (input, mut tag) = if encrypt {
        (data, [0u8; 16])
    } else {
        ensure!(data.len() >= 16, "browser frame authentication tag missing");
        let (input, raw_tag) = data.split_at(data.len() - 16);
        (input, raw_tag.try_into()?)
    };
    let mut info = BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
        cbSize: std::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as u32,
        dwInfoVersion: BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION,
        pbNonce: nonce.as_ptr().cast_mut(),
        cbNonce: nonce.len() as u32,
        pbAuthData: aad.as_ptr().cast_mut(),
        cbAuthData: u32::try_from(aad.len())?,
        pbTag: tag.as_mut_ptr(),
        cbTag: tag.len() as u32,
        ..Default::default()
    };
    let mut output = vec![0u8; input.len()];
    let mut written = 0;
    let status = unsafe {
        let padding = Some(
            (&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<core::ffi::c_void>()
                as *const core::ffi::c_void,
        );
        if encrypt {
            BCryptEncrypt(
                key_handle.0,
                Some(input),
                padding,
                None,
                Some(&mut output),
                &mut written,
                BCRYPT_FLAGS(0),
            )
        } else {
            BCryptDecrypt(
                key_handle.0,
                Some(input),
                padding,
                None,
                Some(&mut output),
                &mut written,
                BCRYPT_FLAGS(0),
            )
        }
    };
    ensure!(
        !status.is_err() && written as usize == output.len(),
        "browser frame encryption or authentication failed"
    );
    if encrypt {
        output.extend_from_slice(&tag);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aes_256_gcm_matches_published_vector_and_rejects_tampering() {
        let key = [0u8; 32];
        let nonce = [0u8; 12];
        let encrypted = crypt(&key, &nonce, b"", &[0u8; 16], true).unwrap();
        assert_eq!(
            encrypted,
            crate::crypto::decode_hex(
                "cea7403d4d606b6e074ec5d3baf39d18d0d1c8a799996bf0265b98b5d48ab919",
                "vector"
            )
            .unwrap()
        );
        assert_eq!(
            crypt(&key, &nonce, b"", &encrypted, false).unwrap(),
            vec![0; 16]
        );
        assert!(crypt(&key, &nonce, b"wrong", &encrypted, false).is_err());
    }
}
