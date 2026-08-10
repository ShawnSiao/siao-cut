use super::error::AiError;

const CREDENTIAL_PREFIX: &str = "app.siaocut.desktop/ai-service/";
const MAX_CREDENTIAL_BYTES: usize = 2_560;

pub trait CredentialStore: Send + Sync {
    fn read(&self, service_id: &str) -> Result<Option<String>, AiError>;
    fn write(&self, service_id: &str, secret: &str) -> Result<(), AiError>;
    fn delete(&self, service_id: &str) -> Result<(), AiError>;
}

#[derive(Default)]
pub struct WindowsCredentialStore;

fn target_name(service_id: &str) -> Result<String, AiError> {
    if service_id.is_empty()
        || service_id.len() > 128
        || !service_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err(AiError::Validation("AI 服务标识无效".to_owned()));
    }
    Ok(format!("{CREDENTIAL_PREFIX}{service_id}"))
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
impl CredentialStore for WindowsCredentialStore {
    fn read(&self, service_id: &str) -> Result<Option<String>, AiError> {
        use std::{ptr, slice};
        use windows_sys::Win32::{
            Foundation::{ERROR_NOT_FOUND, GetLastError},
            Security::Credentials::{CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW},
        };

        let target = wide_null(&target_name(service_id)?);
        let mut credential: *mut CREDENTIALW = ptr::null_mut();
        let success = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
        if success == 0 {
            return if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                Ok(None)
            } else {
                Err(AiError::CredentialRead)
            };
        }
        if credential.is_null() {
            return Err(AiError::CredentialRead);
        }
        let value = unsafe {
            let credential_ref = &*credential;
            let bytes = slice::from_raw_parts(
                credential_ref.CredentialBlob,
                credential_ref.CredentialBlobSize as usize,
            );
            String::from_utf8(bytes.to_vec()).map_err(|_| AiError::CredentialRead)
        };
        unsafe { CredFree(credential.cast()) };
        value.map(Some)
    }

    fn write(&self, service_id: &str, secret: &str) -> Result<(), AiError> {
        use windows_sys::Win32::Security::Credentials::{
            CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredWriteW,
        };

        let bytes = secret.as_bytes();
        if bytes.is_empty() || bytes.len() > MAX_CREDENTIAL_BYTES {
            return Err(AiError::Validation("API Key 长度无效".to_owned()));
        }
        let mut target = wide_null(&target_name(service_id)?);
        let mut username = wide_null("SiaoCut");
        let mut blob = bytes.to_vec();
        let mut credential = unsafe { std::mem::zeroed::<CREDENTIALW>() };
        credential.Type = CRED_TYPE_GENERIC;
        credential.TargetName = target.as_mut_ptr();
        credential.CredentialBlobSize = blob.len() as u32;
        credential.CredentialBlob = blob.as_mut_ptr();
        credential.Persist = CRED_PERSIST_LOCAL_MACHINE;
        credential.UserName = username.as_mut_ptr();
        let success = unsafe { CredWriteW(&credential, 0) };
        blob.fill(0);
        if success == 0 {
            Err(AiError::CredentialWrite)
        } else {
            Ok(())
        }
    }

    fn delete(&self, service_id: &str) -> Result<(), AiError> {
        use windows_sys::Win32::{
            Foundation::{ERROR_NOT_FOUND, GetLastError},
            Security::Credentials::{CRED_TYPE_GENERIC, CredDeleteW},
        };

        let target = wide_null(&target_name(service_id)?);
        let success = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
        if success != 0 || unsafe { GetLastError() } == ERROR_NOT_FOUND {
            Ok(())
        } else {
            Err(AiError::CredentialDelete)
        }
    }
}

#[cfg(not(windows))]
impl CredentialStore for WindowsCredentialStore {
    fn read(&self, _service_id: &str) -> Result<Option<String>, AiError> {
        Err(AiError::CredentialUnsupported)
    }
    fn write(&self, _service_id: &str, _secret: &str) -> Result<(), AiError> {
        Err(AiError::CredentialUnsupported)
    }
    fn delete(&self, _service_id: &str) -> Result<(), AiError> {
        Err(AiError::CredentialUnsupported)
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;
    use std::{collections::HashMap, sync::Mutex};

    #[derive(Default)]
    pub struct MemoryCredentialStore {
        values: Mutex<HashMap<String, String>>,
    }

    impl CredentialStore for MemoryCredentialStore {
        fn read(&self, service_id: &str) -> Result<Option<String>, AiError> {
            Ok(self
                .values
                .lock()
                .expect("credential lock")
                .get(service_id)
                .cloned())
        }
        fn write(&self, service_id: &str, secret: &str) -> Result<(), AiError> {
            self.values
                .lock()
                .expect("credential lock")
                .insert(service_id.to_owned(), secret.to_owned());
            Ok(())
        }
        fn delete(&self, service_id: &str) -> Result<(), AiError> {
            self.values
                .lock()
                .expect("credential lock")
                .remove(service_id);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_target_is_scoped_and_rejects_paths() {
        assert_eq!(
            target_name("service-123").unwrap(),
            "app.siaocut.desktop/ai-service/service-123"
        );
        assert!(target_name("../secret").is_err());
    }
}
