// Win32 API wrappers: Credential Manager, Named Mutexes, Shared Memory IPC.

use std::fs;
use crate::types::{CacheData, QuotaItem, VcsInfo};
use crate::path::get_antigravity_dir;

// --- Helper Functions for String/Byte Conversion in IPC ----------------------

fn bytes_to_str(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches('\0')
        .to_string()
}

fn str_to_bytes<const N: usize>(s: &str) -> [u8; N] {
    let mut bytes = [0u8; N];
    let s_bytes = s.as_bytes();
    let len = s_bytes.len().min(N);
    bytes[..len].copy_from_slice(&s_bytes[..len]);
    bytes
}

// --- Process Status ----------------------------------------------------------

pub fn is_pid_alive(pid: u32) -> bool {
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ACCESS_DENIED};
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if !handle.is_null() {
            CloseHandle(handle);
            true
        } else {
            let err = GetLastError();
            err == ERROR_ACCESS_DENIED
        }
    }
}

// --- Credential Manager ------------------------------------------------------

fn read_windows_credential(target: &str) -> Option<String> {
    use std::ptr;
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CRED_TYPE_GENERIC, CREDENTIALW,
    };

    let target_wide: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
    let mut cred_ptr: *mut CREDENTIALW = ptr::null_mut();

    unsafe {
        let res = CredReadW(
            target_wide.as_ptr(),
            CRED_TYPE_GENERIC,
            0,
            &mut cred_ptr,
        );

        if res != 0 && !cred_ptr.is_null() {
            let cred = &*cred_ptr;
            if cred.CredentialBlobSize > 0 && !cred.CredentialBlob.is_null() {
                let blob_slice = std::slice::from_raw_parts(
                    cred.CredentialBlob,
                    cred.CredentialBlobSize as usize,
                );

                let token_str = if let Ok(s) = String::from_utf8(blob_slice.to_vec()) {
                    Some(s)
                } else {
                    let u16_slice: Vec<u16> = blob_slice
                        .chunks_exact(2)
                        .map(|c| u16::from_ne_bytes([c[0], c[1]]))
                        .collect();
                    String::from_utf16(&u16_slice).ok()
                };

                CredFree(cred_ptr as *mut _);
                return token_str;
            }
            CredFree(cred_ptr as *mut _);
        }
    }
    None
}

pub fn get_access_token() -> Option<String> {
    if let Some(raw_cred) = read_windows_credential("gemini:antigravity")
        .or_else(|| read_windows_credential("LegacyGeneric:target=gemini:antigravity"))
    {
        if let Ok(parsed_json) = serde_json::from_str::<serde_json::Value>(&raw_cred) {
            let access_token = parsed_json
                .get("token")
                .and_then(|t| t.get("access_token"))
                .or_else(|| parsed_json.get("access_token"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if let Some(tok) = access_token {
                return Some(tok);
            }
        }
    }

    let root = get_antigravity_dir();
    let oauth_path = root.join("antigravity-oauth-token");
    if let Ok(data) = fs::read_to_string(oauth_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
            if let Some(tok) = v
                .get("token")
                .and_then(|t| t.get("access_token"))
                .and_then(|s| s.as_str())
            {
                return Some(tok.to_string());
            }
        }
    }
    if let Some(parent) = root.parent() {
        let parent_oauth = parent.join("oauth_creds.json");
        if let Ok(data) = fs::read_to_string(parent_oauth) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                if let Some(tok) = v.get("access_token").and_then(|s| s.as_str()) {
                    return Some(tok.to_string());
                }
            }
        }
    }
    None
}

// --- Named Mutex -------------------------------------------------------------

pub struct NamedMutex {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

impl NamedMutex {
    pub fn is_active(name: &str) -> bool {
        use windows_sys::Win32::System::Threading::OpenMutexW;
        use windows_sys::Win32::Foundation::CloseHandle;
        let name_wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        unsafe {
            let handle = OpenMutexW(0x0001, 0, name_wide.as_ptr());
            if !handle.is_null() {
                CloseHandle(handle);
                true
            } else {
                false
            }
        }
    }

    pub fn acquire(name: &str) -> Option<Self> {
        use windows_sys::Win32::System::Threading::CreateMutexW;
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
        let name_wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 0, name_wide.as_ptr());
            if handle.is_null() {
                return None;
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(handle);
                return None;
            }
            Some(NamedMutex { handle })
        }
    }
}

impl Drop for NamedMutex {
    fn drop(&mut self) {
        use windows_sys::Win32::System::Threading::ReleaseMutex;
        use windows_sys::Win32::Foundation::CloseHandle;
        unsafe {
            ReleaseMutex(self.handle);
            CloseHandle(self.handle);
        }
    }
}

// --- Shared Memory IPC -------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SharedQuotaItem {
    pub id: [u8; 64],
    pub display_name: [u8; 64],
    pub remaining_fraction: f64,
    pub reset_time: [u8; 64],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SharedVcsInfo {
    pub cwd: [u8; 260],
    pub branch: [u8; 64],
    pub dirty: u8,
    pub ahead: u32,
    pub behind: u32,
    pub modified: u32,
    pub last_checked: u64,
    pub head_mtime: u64,
    pub index_mtime: u64,
    pub remote_web_url: [u8; 256],
    pub insertions: u32,
    pub deletions: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SharedCacheData {
    pub magic: u32,
    pub version: u32,
    pub seq: u32,
    pub last_refreshed: u64,
    pub quota_count: u32,
    pub quotas: [SharedQuotaItem; 16],
    pub has_vcs: u8,
    pub vcs: SharedVcsInfo,
    pub needs_login: u8,
}

impl SharedCacheData {
    pub fn to_cache_data(&self) -> CacheData {
        let quota = self.quotas[..self.quota_count.min(16) as usize]
            .iter()
            .map(|q| {
                let rt = bytes_to_str(&q.reset_time);
                QuotaItem {
                    id: bytes_to_str(&q.id),
                    display_name: bytes_to_str(&q.display_name),
                    remaining_fraction: q.remaining_fraction,
                    reset_time: if rt.is_empty() { None } else { Some(rt) },
                }
            })
            .collect();

        let vcs = (self.has_vcs != 0).then(|| VcsInfo {
            cwd: bytes_to_str(&self.vcs.cwd),
            branch: bytes_to_str(&self.vcs.branch),
            dirty: self.vcs.dirty != 0,
            ahead: self.vcs.ahead,
            behind: self.vcs.behind,
            modified: self.vcs.modified,
            last_checked: self.vcs.last_checked,
            head_mtime: if self.vcs.head_mtime == 0 { None } else { Some(self.vcs.head_mtime) },
            index_mtime: if self.vcs.index_mtime == 0 { None } else { Some(self.vcs.index_mtime) },
            remote_web_url: {
                let s = bytes_to_str(&self.vcs.remote_web_url);
                if s.is_empty() { None } else { Some(s) }
            },
            insertions: self.vcs.insertions,
            deletions: self.vcs.deletions,
        });

        let needs_login = match self.needs_login {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        };

        CacheData {
            quota,
            vcs,
            last_refreshed: self.last_refreshed,
            token_hash: None,
            needs_login,
        }
    }

    pub fn from_cache_data(data: &CacheData) -> Self {
        let mut quotas = [SharedQuotaItem {
            id: [0; 64],
            display_name: [0; 64],
            remaining_fraction: 0.0,
            reset_time: [0; 64],
        }; 16];

        let quota_count = data.quota.len().min(16);
        for i in 0..quota_count {
            let q = &data.quota[i];
            quotas[i] = SharedQuotaItem {
                id: str_to_bytes(&q.id),
                display_name: str_to_bytes(&q.display_name),
                remaining_fraction: q.remaining_fraction,
                reset_time: q.reset_time.as_deref().map(str_to_bytes).unwrap_or([0; 64]),
            };
        }

        let mut vcs_info = SharedVcsInfo {
            cwd: [0; 260],
            branch: [0; 64],
            dirty: 0,
            ahead: 0,
            behind: 0,
            modified: 0,
            last_checked: 0,
            head_mtime: 0,
            index_mtime: 0,
            remote_web_url: [0; 256],
            insertions: 0,
            deletions: 0,
        };

        let has_vcs = if let Some(ref v) = data.vcs {
            vcs_info = SharedVcsInfo {
                cwd: str_to_bytes(&v.cwd),
                branch: str_to_bytes(&v.branch),
                dirty: if v.dirty { 1 } else { 0 },
                ahead: v.ahead,
                behind: v.behind,
                modified: v.modified,
                last_checked: v.last_checked,
                head_mtime: v.head_mtime.unwrap_or(0),
                index_mtime: v.index_mtime.unwrap_or(0),
                remote_web_url: v.remote_web_url.as_deref().map(str_to_bytes).unwrap_or([0; 256]),
                insertions: v.insertions,
                deletions: v.deletions,
            };
            1
        } else {
            0
        };

        let needs_login_val = match data.needs_login {
            Some(true) => 1,
            Some(false) => 2,
            None => 0,
        };

        SharedCacheData {
            magic: 0x41475953,
            version: 6,
            seq: 0,
            last_refreshed: data.last_refreshed,
            quota_count: quota_count as u32,
            quotas,
            has_vcs,
            vcs: vcs_info,
            needs_login: needs_login_val,
        }
    }
}

pub fn read_shared_cache() -> Option<CacheData> {
    use windows_sys::Win32::System::Memory::{
        OpenFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_READ,
    };
    use windows_sys::Win32::Foundation::CloseHandle;

    let name: Vec<u16> = "Local\\AgyStatuslineSharedCache"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        let handle = OpenFileMappingW(FILE_MAP_READ, 0, name.as_ptr());
        if handle.is_null() {
            return None;
        }
        let view = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, std::mem::size_of::<SharedCacheData>());
        if view.Value.is_null() {
            CloseHandle(handle);
            return None;
        }
        let shared_data = &*(view.Value as *const SharedCacheData);
        let mut result = None;
        if shared_data.magic == 0x41475953 && shared_data.version == 6 {
            let mut attempts = 0;
            loop {
                let seq1 = unsafe { std::ptr::read_volatile(&shared_data.seq) };
                std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
                let cached = shared_data.to_cache_data();
                std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
                let seq2 = unsafe { std::ptr::read_volatile(&shared_data.seq) };
                if seq1 == seq2 && seq1 % 2 == 0 {
                    result = Some(cached);
                    break;
                }
                attempts += 1;
                if attempts >= 3 {
                    break;
                }
                std::thread::yield_now();
            }
        }
        UnmapViewOfFile(view);
        CloseHandle(handle);
        result
    }
}

pub fn write_shared_cache(data: &CacheData) -> bool {
    use windows_sys::Win32::System::Memory::{
        CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, PAGE_READWRITE, FILE_MAP_WRITE,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use std::ptr;

    let name: Vec<u16> = "Local\\AgyStatuslineSharedCache"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let size = std::mem::size_of::<SharedCacheData>();
    unsafe {
        let handle = CreateFileMappingW(
            INVALID_HANDLE_VALUE,
            ptr::null(),
            PAGE_READWRITE,
            0,
            size as u32,
            name.as_ptr(),
        );
        if handle.is_null() {
            return false;
        }
        let view = MapViewOfFile(handle, FILE_MAP_WRITE, 0, 0, size);
        if view.Value.is_null() {
            CloseHandle(handle);
            return false;
        }
        let shared_data = view.Value as *mut SharedCacheData;
        let mut current_seq = unsafe { std::ptr::read_volatile(&mut (*shared_data).seq) };
        if current_seq % 2 == 0 {
            current_seq = current_seq.wrapping_add(1);
        } else {
            current_seq = current_seq.wrapping_add(2);
        }
        unsafe {
            std::ptr::write_volatile(&mut (*shared_data).seq, current_seq);
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);

        let mut new_data = SharedCacheData::from_cache_data(data);
        new_data.seq = current_seq;
        unsafe {
            std::ptr::write(shared_data, new_data);
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);

        unsafe {
            std::ptr::write_volatile(&mut (*shared_data).seq, current_seq.wrapping_add(1));
        }
        UnmapViewOfFile(view);
        CloseHandle(handle);
        true
    }
}
