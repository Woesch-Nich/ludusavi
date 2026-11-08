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
        if actual != expected {
            return Err(format!("checksum mismatch: expected {expected}, got {actual}").into());
        }

        let staging = app_dir().joined("update-staging").as_std_path_buf()?;
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
/// The current exe is kept as `<exe>.prev`. A detached helper waits for this
/// process to exit, swaps the file (retrying while another Ludusavi process —
/// e.g. a scheduled backup — still holds it), restarts with the same arguments,
/// and rolls back to the previous exe if the new one exits immediately.
#[cfg(target_os = "windows")]
pub fn apply_and_restart(staged: &StagedUpdate) -> Result<(), String> {
    use base64::prelude::*;

    let target = std::env::current_exe().map_err(|e| e.to_string())?;
    let backup = std::path::PathBuf::from(format!("{}.prev", target.display()));
    std::fs::copy(&target, &backup).map_err(|e| format!("could not back up the current exe: {e}"))?;

    let working_dir = target
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| ".".to_string());
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args_literal = if args.is_empty() {
        String::new()
    } else {
        format!(
            "-ArgumentList {}",
            args.iter()
                .map(|a| ps_quote(a))
                .collect::<Vec<_>>()
                .join(",")
        )
    };

    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$targetPid = {pid}
$new = {new}
$target = {target}
$backup = {backup}
$wd = {wd}
if ($targetPid -gt 0) {{
    Wait-Process -Id $targetPid -Timeout 120 -ErrorAction SilentlyContinue
}}
$ok = $false
for ($i = 0; $i -lt 90; $i++) {{
    try {{
        Copy-Item -LiteralPath $new -Destination $target -Force
        $ok = $true
        break
    }} catch {{
        Start-Sleep -Seconds 2
    }}
}}
if (-not $ok) {{ exit 1 }}
$p = Start-Process -FilePath $target {args_literal} -WorkingDirectory $wd -PassThru
Start-Sleep -Seconds 8
if ($p.HasExited) {{
    Copy-Item -LiteralPath $backup -Destination $target -Force
    Start-Process -FilePath $target {args_literal} -WorkingDirectory $wd
}}
Remove-Item -LiteralPath $new -Force -ErrorAction SilentlyContinue
"#,
        pid = std::process::id(),
        new = ps_quote(&staged.new_exe.display().to_string()),
        target = ps_quote(&target.display().to_string()),
        backup = ps_quote(&backup.display().to_string()),
        wd = ps_quote(&working_dir),
        args_literal = args_literal,
    );

    let utf16: Vec<u8> = script.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &BASE64_STANDARD.encode(utf16)])
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn apply_and_restart(_staged: &StagedUpdate) -> Result<(), String> {
    Err("automatic installation is currently only supported on Windows".to_string())
}

#[cfg(target_os = "windows")]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}