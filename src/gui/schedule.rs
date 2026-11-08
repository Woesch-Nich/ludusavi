//! OS-backed daily backup scheduling.
//!
//! The GUI setting is authoritative, but the actual trigger is registered with
//! the operating system so backups still run when the Ludusavi window is closed.

use ludusavi::resource::config::Config;

/// Returns true when the operating system is managing the schedule, meaning the
/// in-process GUI timer should stay out of the way to avoid duplicate backups.
pub fn apply(config: &Config) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        windows::apply(config)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = config;
        Ok(false)
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use base64::prelude::*;
    use ludusavi::{prelude::app_dir, resource::config::Config};

    const TASK_NAME: &str = "Ludusavi Auto Backup";

    pub fn apply(config: &Config) -> Result<bool, String> {
        if !config.schedule.enabled {
            delete_task()?;
            return Ok(false);
        }

        let Some(time) = config.schedule.parsed_time() else {
            // Avoid leaving a stale OS task behind if the user entered an invalid time.
            delete_task()?;
            return Ok(false);
        };

        register_task(&time.format("%H:%M").to_string())?;
        Ok(true)
    }

    fn register_task(time: &str) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_string = exe.display().to_string();
        let working_dir = exe
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| ".".to_string());
        let config_dir = app_dir().render();

        // The scheduled action uses the same config directory as this GUI instance.
        let arguments = format!("--config \"{}\" backup --force", escape_cmd_argument(&config_dir));

        let script = format!(
            r#"
$ErrorActionPreference = 'Stop'
$name = {name}
$exe = {exe}
$workingDir = {working_dir}
$arguments = {arguments}
$time = {time}
$action = New-ScheduledTaskAction -Execute $exe -Argument $arguments -WorkingDirectory $workingDir
$trigger = New-ScheduledTaskTrigger -Daily -At $time
$settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Hours 2)
$principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName $name -Action $action -Trigger $trigger -Settings $settings -Principal $principal -Description 'Daily Ludusavi save backup, managed from the Ludusavi settings screen.' -Force | Out-Null
"#,
            name = ps_quote(TASK_NAME),
            exe = ps_quote(&exe_string),
            working_dir = ps_quote(&working_dir),
            arguments = ps_quote(&arguments),
            time = ps_quote(time),
        );

        run_powershell(&script)
    }

    fn delete_task() -> Result<(), String> {
        let script = format!(
            r#"
$ErrorActionPreference = 'Stop'
$name = {name}
$task = Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue
if ($task) {{
    Unregister-ScheduledTask -TaskName $name -Confirm:$false
}}
"#,
            name = ps_quote(TASK_NAME),
        );

        run_powershell(&script)
    }

    fn run_powershell(script: &str) -> Result<(), String> {
        let utf16: Vec<u8> = script.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        let encoded = BASE64_STANDARD.encode(utf16);

        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
            .output()
            .map_err(|e| e.to_string())?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Err(if stderr.is_empty() { stdout } else { stderr })
        }
    }

    fn ps_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "''"))
    }

    fn escape_cmd_argument(value: &str) -> String {
        value.replace('"', "\\\"")
    }
}
