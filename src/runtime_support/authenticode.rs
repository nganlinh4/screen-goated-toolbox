use std::os::windows::ffi::OsStrExt as _;
use std::path::Path;

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Security::Cryptography::{CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_NONE, WTD_REVOKE_NONE,
    WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTD_UICONTEXT_INSTALL,
    WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData,
    WinVerifyTrust,
};
use windows::core::{PCWSTR, PWSTR};

pub(super) fn verify_publisher(path: &Path, expected: &str) -> Result<()> {
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        hFile: HANDLE::default(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut trust = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        pPolicyCallbackData: std::ptr::null_mut(),
        pSIPClientData: std::ptr::null_mut(),
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_VERIFY,
        hWVTStateData: HANDLE::default(),
        pwszURLReference: PWSTR::null(),
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL | WTD_REVOCATION_CHECK_NONE,
        dwUIContext: WTD_UICONTEXT_INSTALL,
        pSignatureSettings: std::ptr::null_mut(),
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe { WinVerifyTrust(HWND::default(), &mut action, (&raw mut trust).cast()) };
    let result = if status != 0 {
        Err(anyhow::anyhow!(
            "WebView2 bootstrapper Authenticode verification failed ({status:#x})"
        ))
    } else {
        signer_name(&trust).and_then(|publisher| {
            if publisher == expected {
                Ok(())
            } else {
                bail!("WebView2 bootstrapper publisher is not {expected}")
            }
        })
    };
    trust.dwStateAction = WTD_STATEACTION_CLOSE;
    let _ = unsafe { WinVerifyTrust(HWND::default(), &mut action, (&raw mut trust).cast()) };
    result
}

fn signer_name(trust: &WINTRUST_DATA) -> Result<String> {
    let provider = unsafe { WTHelperProvDataFromStateData(trust.hWVTStateData) };
    if provider.is_null() {
        bail!("WebView2 signature provider data is unavailable");
    }
    let signer = unsafe { WTHelperGetProvSignerFromChain(provider, 0, false, 0) };
    if signer.is_null() {
        bail!("WebView2 signer is unavailable");
    }
    let certificate = unsafe { WTHelperGetProvCertFromChain(signer, 0) };
    if certificate.is_null() {
        bail!("WebView2 signing certificate is unavailable");
    }
    let context = unsafe { (*certificate).pCert };
    if context.is_null() {
        bail!("WebView2 signing certificate context is unavailable");
    }
    let length =
        unsafe { CertGetNameStringW(context, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, None) };
    if length <= 1 || length > 512 {
        bail!("WebView2 publisher name is invalid");
    }
    let mut buffer = vec![0_u16; length as usize];
    let written = unsafe {
        CertGetNameStringW(
            context,
            CERT_NAME_SIMPLE_DISPLAY_TYPE,
            0,
            None,
            Some(&mut buffer),
        )
    };
    if written != length {
        bail!("WebView2 publisher name could not be read");
    }
    String::from_utf16(&buffer[..buffer.len() - 1]).context("WebView2 publisher name is invalid")
}
