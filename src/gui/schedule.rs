//! OS-backed daily backup scheduling.
//!
//! The GUI setting is authoritative, but the actual trigger is registered with
//! the operating system so backups still run when the Ludusavi window is closed.
//!
//! On Windows this uses plain `schtasks.exe` with a readable task XML file —
//! no PowerShell, no encoded commands.

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

        register_task(&time)?;
        Ok(true)
    }

    fn register_task(time: &chrono::NaiveTime) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_string = exe.display().to_string();
        let working_dir = exe
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| ".".to_string());
        let config_dir = app_dir().render();
        let arguments = format!("--config \"{}\" backup --force", config_dir.replace('"', ""));

        let user = format!(
            "{}\\{}",
            std::env::var("USERDOMAIN").unwrap_or_else(|_| ".".to_string()),
            std::env::var("USERNAME").map_err(|_| "USERNAME environment variable not set".to_string())?
        );
        let start_boundary = chrono::Local::now()
            .date_naive()
            .and_time(*time)
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();

        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Daily Ludusavi save backup, managed from the Ludusavi settings screen.</Description>
  </RegistrationInfo>
  <Triggers>
    <CalendarTrigger>
      <StartBoundary>{start_boundary}</StartBoundary>
      <Enabled>true</Enabled>
      <ScheduleByDay>
        <DaysInterval>1</DaysInterval>
      </ScheduleByDay>
    </CalendarTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT2H</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{command}</Command>
      <Arguments>{arguments}</Arguments>
      <WorkingDirectory>{working_dir}</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
"#,
            start_boundary = start_boundary,
            user = xml_escape(&user),
            command = xml_escape(&exe_string),
            arguments = xml_escape(&arguments),
            working_dir = xml_escape(&working_dir),
        );

        let xml_path = std::env::temp_dir().join("ludusavi-task.xml");
        std::fs::write(&xml_path, xml).map_err(|e| e.to_string())?;
        let result = run_schtasks(&[
            "/Create",
            "/TN",
            TASK_NAME,
            "/XML",
            &xml_path.display().to_string(),
            "/F",
        ]);
        let _ = std::fs::remove_file(&xml_path);
        result
    }

    fn delete_task() -> Result<(), String> {
        match run_schtasks(&["/Delete", "/TN", TASK_NAME, "/F"]) {
            Ok(()) => Ok(()),
            Err(e) if e.contains("cannot find") => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn run_schtasks(args: &[&str]) -> Result<(), String> {
        let output = std::process::Command::new("schtasks")
            .args(args)
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

    fn xml_escape(value: &str) -> String {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }
}