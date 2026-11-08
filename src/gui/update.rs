//! True auto-update for the fork: download the latest fork release, verify its
//! SHA-256, stage the new executable, then swap it in and restart.
//!
//! Only fork releases are ever installed (see `metadata::FORK_REPO`), so fork
//! features such as the daily backup scheduler survive updates.

use ludusavi::{
    metadata::ForkRelease,
    prelude::{Security, app_dir},
};

#[derive(Debug, Clone)]
pub struct StagedUpdate {
    pub tag: String,
    pub new_exe: std::path::PathBuf,
}

/// Download the release zip, verify it against the published `.sha256` asset,
/// and extract `ludusavi.exe` into a staging directory.
pub async fn download_and_stage(release: &ForkRelease, security: Security) -> Result<StagedUpdate, String> {
    async fn run(release: &ForkRelease, security: Security) -> Result<StagedUpdate, ludusavi::prelude::AnyError> {
        use sha2::{Digest, Sha256};

        let sha256_url = release
            .sha256_url
            .as_ref()
            .ok_or("release is missing its .sha256 asset; refusing to auto-install")?;

        let client = ludusavi::prelude::get_reqwest_client(security);
        let user_agent = &*ludusavi::prelude::USER_AGENT;

        let expected = client
            .get(sha256_url)
            .header(reqwest::header::USER_AGENT, user_agent)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let expected = expected.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
        log::debug!("Update: expected sha256={expected}");
        if expected.len() != 64 || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("invalid .sha256 asset content".into());
        }

        let zip_bytes = client
            .get(&release.zip_url)
            .header(reqwest::header::USER_AGENT, user_agent)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        let actual = format!("{:x}", Sha256::digest(&zip_bytes));
        log::debug!("Update: downloaded {} bytes, sha256={actual}", zip_bytes.len());
        if actual != expected {
            return Err(format!("checksum mismatch: expected {expected}, got {actual}").into());
        }

        let staging = app_dir().joined("update-staging").as_std_path_buf()?;
        log::debug!("Update: checksum OK, staging at {}", staging.display());
        std::fs::create_dir_all(&staging)?;

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))?;
        let mut exe_index = None;
        for i in 0..archive.len() {
            let entry = archive.by_index(i)?;
            if entry.name()?.ends_with("ludusavi.exe") {
                exe_index = Some(i);
                break;
            }
        }
        let index = exe_index.ok_or("release zip does not contain ludusavi.exe")?;
        let mut entry = archive.by_index(index)?;
        let new_exe = staging.join("ludusavi.exe");
        let mut out = std::fs::File::create(&new_exe)?;
        std::io::copy(&mut entry, &mut out)?;

        Ok(StagedUpdate {
            tag: release.tag.clone(),
            new_exe,
        })
    }

    run(release, security).await.map_err(|e| e.to_string())
}

/// Replace the running executable with the staged one and restart.
///
/// Uses the native Windows rename trick: a running executable may be renamed
/// (though not deleted/overwritten). The app renames itself to `<exe>.old`,
/// moves the verified new exe into place, and spawns it with the same
/// arguments. If the new process exits immediately, the previous exe is
/// restored and restarted. No helper scripts or PowerShell are involved.
#[cfg(target_os = "windows")]
pub fn apply_and_restart(staged: &StagedUpdate) -> Result<(), String> {
    use std::process::Command;

    let target = std::env::current_exe().map_err(|e| e.to_string())?;
    let old = std::path::PathBuf::from(format!("{}.old", target.display()));
    let working_dir = target.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Leftover from a previous successful update: best-effort removal.
    let _ = std::fs::remove_file(&old);

    // Rename the running exe out of the way (allowed on Windows), then move
    // the staged exe into its place. Roll back if the move fails.
    std::fs::rename(&target, &old).map_err(|e| format!("could not rename the running exe: {e}"))?;
    if let Err(e) = std::fs::rename(&staged.new_exe, &target) {
        let _ = std::fs::rename(&old, &target);
        return Err(format!("could not move the new exe into place: {e}"));
    }

    let spawn = |exe: &std::path::Path| -> Result<std::process::Child, String> {
        Command::new(exe)
            .args(&args)
            .current_dir(&working_dir)
            .spawn()
            .map_err(|e| e.to_string())
    };

    let mut child = match spawn(&target) {
        Ok(c) => c,
        Err(e) => {
            // Restore the previous exe; this process keeps running it, so no restart here.
            let _ = std::fs::remove_file(&target);
            let _ = std::fs::rename(&old, &target);
            return Err(format!("new exe failed to start (rolled back): {e}"));
        }
    };

    // Give the new build a few seconds; if it dies immediately, roll back.
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        match child.try_wait() {
            Ok(Some(status)) => {
                log::warn!("Updated exe exited immediately ({status}); rolling back");
                let _ = std::fs::remove_file(&target);
                let _ = std::fs::rename(&old, &target);
                return Err(format!("new exe exited immediately ({status}); rolled back"));
            }
            Ok(None) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

/// Remove the previous executable left behind by a successful update.
/// Best-effort; called once at startup.
pub fn cleanup_previous_update() {
    #[cfg(target_os = "windows")]
    if let Ok(exe) = std::env::current_exe() {
        let old = std::path::PathBuf::from(format!("{}.old", exe.display()));
        let _ = std::fs::remove_file(old);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn apply_and_restart(_staged: &StagedUpdate) -> Result<(), String> {
    Err("automatic installation is currently only supported on Windows".to_string())
}
