// Background refresh: quota API fetch, git status query, and cache persistence.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::types::{CacheData, QuotaItem, VcsInfo};
use crate::crypto::sha256_hex;
use crate::path::{resolve_antigravity_path, get_configs_last_modified_time, get_git_branch_fast};
use crate::platform::{get_access_token, NamedMutex, write_shared_cache};

pub fn run_background_refresh(cwd_force: Option<String>) {
    #[cfg(windows)]
    let _mutex = match NamedMutex::acquire("Local\\AgyStatuslineRefreshMutex") {
        Some(m) => m,
        None => return,
    };

    let status_cache_path = resolve_antigravity_path("statusline-cache.json");
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

    let mut existing_cache = CacheData::default();
    if let Ok(cache_str) = fs::read_to_string(&status_cache_path) {
        if let Ok(parsed) = serde_json::from_str::<CacheData>(&cache_str) {
            existing_cache = parsed;
        }
    }

    let last_config_update = get_configs_last_modified_time();
    let mut cache_modified_secs = 0u64;
    if let Ok(metadata) = fs::metadata(&status_cache_path) {
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                cache_modified_secs = duration.as_secs();
            }
        }
    }

    let token_opt = get_access_token();
    let current_token_hash = token_opt.as_ref().map(|t| sha256_hex(t));
    let token_changed = current_token_hash != existing_cache.token_hash;

    if token_changed {
        existing_cache.needs_login = None;
    }

    let quota_age = now.saturating_sub(existing_cache.last_refreshed);
    let need_quota_fetch = (existing_cache.quota.is_empty()
        || quota_age > 120
        || last_config_update > cache_modified_secs
        || token_changed)
        && token_opt.is_some();

    if token_opt.is_none() {
        existing_cache.token_hash = None;
        existing_cache.needs_login = Some(true);
        existing_cache.last_refreshed = now;
        existing_cache.quota.clear();
    } else if need_quota_fetch {
        if let Some(ref token) = token_opt {
            fetch_quota(token, &current_token_hash, now, &mut existing_cache);
        }
    }

    // Git status
    if let Some(ref cwd) = cwd_force {
        collect_git_status(cwd, now, &mut existing_cache);
    }

    // Persist to file
    let tmp_path = format!("{}.tmp.{}", status_cache_path.to_string_lossy(), std::process::id());
    if let Ok(serialized) = serde_json::to_string(&existing_cache) {
        if fs::write(&tmp_path, serialized).is_ok() {
            let _ = fs::rename(tmp_path, &status_cache_path);
        }
    }

    write_shared_cache(&existing_cache);
}

fn fetch_quota(token: &str, current_token_hash: &Option<String>, now: u64, cache: &mut CacheData) {
    use ureq::tls::{TlsConfig, RootCerts};

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(6)))
        .http_status_as_error(false)
        .user_agent("antigravity-statusline-win11-rs/1.1.0 windows/11")
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into();

    let url = "https://cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels";
    let res = agent
        .post(url)
        .header("Authorization", &format!("Bearer {}", token))
        .send_json(&serde_json::json!({}));

    match res {
        Ok(mut resp) => {
            let status = resp.status();
            if status == 200 {
                if let Ok(json_body) = resp.body_mut().read_json::<serde_json::Value>() {
                    if let Some(models) = json_body.get("models").and_then(|m| m.as_object()) {
                        let mut quota_list: Vec<QuotaItem> = Vec::new();
                        for (key, model_val) in models {
                            if let Some(quota_info) = model_val.get("quotaInfo") {
                                let remaining_fraction = quota_info
                                    .get("remainingFraction")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0);
                                let reset_time = quota_info
                                    .get("resetTime")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());
                                let display_name = model_val
                                    .get("displayName")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(key)
                                    .to_string();

                                quota_list.push(QuotaItem {
                                    id: key.clone(),
                                    display_name,
                                    remaining_fraction,
                                    reset_time,
                                });
                            }
                        }
                        cache.needs_login = Some(false);
                        cache.token_hash = current_token_hash.clone();
                        cache.quota = quota_list;
                        cache.last_refreshed = now;
                        return;
                    }
                }
            } else if status == 401 || status == 403 {
                cache.needs_login = Some(true);
                cache.token_hash = current_token_hash.clone();
                cache.quota.clear();
                cache.last_refreshed = now;
                return;
            }
        }
        _ => {}
    }

    cache.last_refreshed = now;
}

fn collect_git_status(cwd: &str, now: u64, cache: &mut CacheData) {
    if !Path::new(cwd).exists() {
        return;
    }

    let mut git_branch = String::new();
    let mut git_dirty = false;
    let mut git_ahead = 0u32;
    let mut git_behind = 0u32;
    let mut git_modified = 0u32;

    if let Some(branch) = get_git_branch_fast(cwd) {
        git_branch = branch;
        if let Ok(status_out) = Command::new("git")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .args(["status", "--porcelain"])
            .current_dir(cwd)
            .output()
        {
            let clean_status = String::from_utf8_lossy(&status_out.stdout);
            let count = clean_status
                .lines()
                .filter(|l| !l.trim().is_empty())
                .count() as u32;
            git_dirty = count > 0;
            git_modified = count;
        }

        if let Ok(rev_out) = Command::new("git")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .args(["rev-list", "--left-right", "--count", "HEAD...@{u}"])
            .current_dir(cwd)
            .output()
        {
            if rev_out.status.success() {
                let output_str = String::from_utf8_lossy(&rev_out.stdout).trim().to_string();
                let parts: Vec<&str> = output_str.split_whitespace().collect();
                if parts.len() == 2 {
                    if let Ok(a) = parts[0].parse::<u32>() {
                        git_ahead = a;
                    }
                    if let Ok(b) = parts[1].parse::<u32>() {
                        git_behind = b;
                    }
                }
            }
        }
    }

    cache.vcs = Some(VcsInfo {
        cwd: cwd.to_string(),
        branch: git_branch,
        dirty: git_dirty,
        ahead: git_ahead,
        behind: git_behind,
        modified: git_modified,
        last_checked: now,
    });
}
