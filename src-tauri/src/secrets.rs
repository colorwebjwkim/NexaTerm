use base64::{Engine, engine::general_purpose::STANDARD};
use zeroize::Zeroizing;

#[cfg(windows)]
fn transform(input: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    };
    let source = CRYPT_INTEGER_BLOB {
        cbData: input.len().try_into().map_err(|_| "비밀정보 크기 제한")?,
        pbData: input.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: input lives for the call; output is allocated by DPAPI and released with LocalFree.
    unsafe {
        let success = if encrypt {
            CryptProtectData(
                &source,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &source,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if success == 0 {
            return Err("Windows 사용자 보안 저장소를 사용할 수 없습니다".into());
        }
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        std::ptr::write_bytes(output.pbData, 0, output.cbData as usize);
        LocalFree(output.pbData as *mut _);
        Ok(bytes)
    }
}
#[cfg(not(windows))]
fn transform(_: &[u8], _: bool) -> Result<Vec<u8>, String> {
    Err("이 빌드는 Windows 보안 저장소만 지원합니다".into())
}

pub fn protect(secret: &str) -> Result<String, String> {
    if secret.is_empty() || secret.len() > 16384 {
        return Err("비밀번호 길이를 확인하세요".into());
    }
    Ok(STANDARD.encode(transform(secret.as_bytes(), true)?))
}
pub fn reveal(blob: &str) -> Result<Zeroizing<String>, String> {
    let bytes = STANDARD.decode(blob).map_err(|_| "자격 증명 형식 오류")?;
    let plain = Zeroizing::new(transform(&bytes, false)?);
    let text = std::str::from_utf8(&plain)
        .map_err(|_| "자격 증명 복호화 실패")?
        .to_owned();
    Ok(Zeroizing::new(text))
}
#[cfg(test)]
mod tests {
    #[test]
    #[cfg(windows)]
    fn dpapi_roundtrip_and_tamper() {
        let secret = "NexaTerm-fixture-비밀번호-582!";
        let encrypted = super::protect(secret).unwrap();
        assert!(!encrypted.contains(secret));
        assert_eq!(super::reveal(&encrypted).unwrap().as_str(), secret);
        assert!(super::reveal("invalid").is_err());
    }
}
