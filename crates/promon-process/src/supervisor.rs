use std::process::Stdio;
use std::time::Duration;

use chrono::{DateTime, Datelike, Local, Timelike, Utc};
use promon_core::{ManagedProcess, ProcessStatus, PromonError, PromonResult, ResolvedAppSpec};
use promon_logging::ensure_log_paths;
use promon_node_support::resolve_runtime_command;
use promon_platform::{
    force_kill_process_tree, is_process_alive, logs_dir, process_command, terminate_process_tree,
};
use tokio::fs::OpenOptions;
use tokio::process::Command;
use tokio::time::sleep;

use crate::{load_processes, remove_process, save_processes, upsert_process};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyRestartReason {
    MemoryLimit { used_bytes: u64, limit_bytes: u64 },
    Scheduled { rule: String },
}

impl std::fmt::Display for PolicyRestartReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MemoryLimit {
                used_bytes,
                limit_bytes,
            } => write!(
                formatter,
                "memory {used_bytes} exceeded limit {limit_bytes}"
            ),
            Self::Scheduled { rule } => write!(formatter, "scheduled restart matched {rule}"),
        }
    }
}

pub async fn start_app(app: &ResolvedAppSpec) -> PromonResult<ManagedProcess> {
    if let Some(existing) = load_processes()
        .await?
        .into_iter()
        .find(|process| process.name == app.name && is_managed_process_alive(process))
    {
        return Ok(existing);
    }

    let command = resolve_runtime_command(app)?;
    let log_paths = ensure_log_paths(app, logs_dir())
        .await
        .map_err(PromonError::Io)?;
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_paths.out)
        .await
        .map_err(PromonError::Io)?
        .into_std()
        .await;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_paths.err)
        .await
        .map_err(PromonError::Io)?
        .into_std()
        .await;

    let mut child = Command::new(&command.program);
    child
        .args(&command.args)
        .current_dir(&command.cwd)
        .envs(&command.env)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    configure_process_group(&mut child);

    let child = child.spawn().map_err(PromonError::Io)?;
    let pid = child
        .id()
        .ok_or_else(|| PromonError::Process(format!("failed to read pid for {}", app.name)))?;

    let process = ManagedProcess {
        name: app.name.clone(),
        pid,
        status: ProcessStatus::Running,
        cwd: command.cwd.clone(),
        command,
        started_at: Utc::now(),
        out_log: log_paths.out,
        err_log: log_paths.err,
    };
    upsert_process(process.clone()).await?;
    Ok(process)
}

pub fn validate_restart_policy(app: &ResolvedAppSpec) -> PromonResult<()> {
    if let Some(limit) = app.max_memory_restart.as_deref() {
        parse_memory_limit(limit)?;
    }
    if let Some(rule) = app.cron_restart.as_deref() {
        parse_restart_delay(rule)?;
    }
    Ok(())
}

pub fn policy_restart_reason(
    app: &ResolvedAppSpec,
    process: &ManagedProcess,
) -> PromonResult<Option<PolicyRestartReason>> {
    policy_restart_reason_at(app, process, Utc::now(), process_memory_bytes(process.pid))
}

fn policy_restart_reason_at(
    app: &ResolvedAppSpec,
    process: &ManagedProcess,
    now: DateTime<Utc>,
    memory_bytes: u64,
) -> PromonResult<Option<PolicyRestartReason>> {
    if let Some(limit) = app
        .max_memory_restart
        .as_deref()
        .map(parse_memory_limit)
        .transpose()?
    {
        if memory_bytes > limit {
            return Ok(Some(PolicyRestartReason::MemoryLimit {
                used_bytes: memory_bytes,
                limit_bytes: limit,
            }));
        }
    }

    let Some(rule) = app.cron_restart.as_deref() else {
        return Ok(None);
    };

    if rule.split_whitespace().count() >= 5 {
        if cron_restart_due(rule, process.started_at, now)? {
            return Ok(Some(PolicyRestartReason::Scheduled {
                rule: rule.to_string(),
            }));
        }
    } else if now
        .signed_duration_since(process.started_at)
        .to_std()
        .unwrap_or_default()
        >= parse_duration_ms(rule)?
    {
        return Ok(Some(PolicyRestartReason::Scheduled {
            rule: rule.to_string(),
        }));
    }

    Ok(None)
}

pub async fn run_app_foreground(app: &ResolvedAppSpec) -> PromonResult<()> {
    let mut restarts = 0_u32;
    loop {
        let command = resolve_runtime_command(app)?;
        let log_paths = ensure_log_paths(app, logs_dir())
            .await
            .map_err(PromonError::Io)?;
        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_paths.out)
            .await
            .map_err(PromonError::Io)?
            .into_std()
            .await;
        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_paths.err)
            .await
            .map_err(PromonError::Io)?
            .into_std()
            .await;

        let mut command_builder = Command::new(&command.program);
        command_builder
            .args(&command.args)
            .current_dir(&command.cwd)
            .envs(&command.env)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        configure_process_group(&mut command_builder);
        let mut child = command_builder.spawn().map_err(PromonError::Io)?;
        let pid = child
            .id()
            .ok_or_else(|| PromonError::Process(format!("failed to read pid for {}", app.name)))?;
        let started = std::time::Instant::now();
        let memory_limit = app
            .max_memory_restart
            .as_deref()
            .map(parse_memory_limit)
            .transpose()?;
        let interval_restart = app
            .cron_restart
            .as_deref()
            .map(parse_restart_delay)
            .transpose()?;

        let status = loop {
            if let Some(status) = child.try_wait().map_err(PromonError::Io)? {
                break status;
            }

            if let Some(limit) = memory_limit {
                if process_memory_bytes(pid) > limit {
                    terminate_process_tree(pid).await.map_err(PromonError::Io)?;
                    sleep(Duration::from_millis(500)).await;
                    if is_process_alive(pid) {
                        force_kill_process_tree(pid)
                            .await
                            .map_err(PromonError::Io)?;
                    }
                    break child.wait().await.map_err(PromonError::Io)?;
                }
            }

            if let Some(interval) = interval_restart {
                if started.elapsed() >= interval {
                    terminate_process_tree(pid).await.map_err(PromonError::Io)?;
                    sleep(Duration::from_millis(500)).await;
                    if is_process_alive(pid) {
                        force_kill_process_tree(pid)
                            .await
                            .map_err(PromonError::Io)?;
                    }
                    break child.wait().await.map_err(PromonError::Io)?;
                }
            }

            sleep(Duration::from_millis(500)).await;
        };

        if status.success() || !app.restart.autorestart {
            return Ok(());
        }

        restarts += 1;
        if let Some(max) = app.restart.max_restarts {
            if restarts > max {
                return Err(PromonError::Process(format!(
                    "app {} exceeded max_restarts={max}",
                    app.name
                )));
            }
        }

        let delay = app.restart.restart_delay_ms.unwrap_or(1000);
        sleep(Duration::from_millis(delay)).await;
    }
}

fn parse_memory_limit(value: &str) -> PromonResult<u64> {
    let trimmed = value.trim();
    let split = trimmed
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(split);
    let number: u64 = number
        .parse()
        .map_err(|_| PromonError::Config(format!("invalid memory limit: {value}")))?;
    let multiplier = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" => 1024,
        "m" | "mb" => 1024 * 1024,
        "g" | "gb" => 1024 * 1024 * 1024,
        _ => return Err(PromonError::Config(format!("invalid memory unit: {value}"))),
    };
    Ok(number.saturating_mul(multiplier))
}

fn parse_restart_delay(value: &str) -> PromonResult<Duration> {
    if value.split_whitespace().count() >= 5 {
        return next_cron_delay(value);
    }
    parse_duration_ms(value)
}

fn parse_duration_ms(value: &str) -> PromonResult<Duration> {
    let trimmed = value.trim();
    let split = trimmed
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(split);
    let number: u64 = number
        .parse()
        .map_err(|_| PromonError::Config(format!("invalid restart interval: {value}")))?;
    let millis = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "ms" => number,
        "s" | "sec" | "secs" => number * 1000,
        "m" | "min" | "mins" => number * 60 * 1000,
        "h" | "hr" | "hrs" => number * 60 * 60 * 1000,
        _ => {
            return Err(PromonError::Config(format!(
                "cron_restart currently accepts intervals such as 30s, 5m, or 1h: {value}"
            )))
        }
    };
    Ok(Duration::from_millis(millis))
}

fn next_cron_delay(value: &str) -> PromonResult<Duration> {
    let spec = parse_cron_spec(value)?;
    let now = chrono::Local::now();

    for offset in 1..=(366 * 24 * 60) {
        let candidate = now + chrono::Duration::minutes(offset);
        if cron_matches(&spec, candidate) {
            let delay = candidate
                .signed_duration_since(now)
                .to_std()
                .map_err(|_| PromonError::Config(format!("invalid cron_restart: {value}")))?;
            return Ok(delay);
        }
    }

    Err(PromonError::Config(format!(
        "cron_restart has no matching time in the next year: {value}"
    )))
}

struct CronSpec {
    minutes: Vec<u32>,
    hours: Vec<u32>,
    days: Vec<u32>,
    months: Vec<u32>,
    weekdays: Vec<u32>,
    day_is_wildcard: bool,
    weekday_is_wildcard: bool,
}

fn parse_cron_spec(value: &str) -> PromonResult<CronSpec> {
    let fields: Vec<_> = value.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(PromonError::Config(format!(
            "cron_restart cron syntax expects 5 fields: {value}"
        )));
    }

    Ok(CronSpec {
        minutes: parse_cron_field(fields[0], 0, 59)?,
        hours: parse_cron_field(fields[1], 0, 23)?,
        days: parse_cron_field(fields[2], 1, 31)?,
        months: parse_cron_field(fields[3], 1, 12)?,
        weekdays: parse_cron_field(fields[4], 0, 7)?,
        day_is_wildcard: fields[2] == "*",
        weekday_is_wildcard: fields[4] == "*",
    })
}

fn cron_restart_due(
    rule: &str,
    started_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> PromonResult<bool> {
    let spec = parse_cron_spec(rule)?;
    let started = started_at.with_timezone(&Local);
    let now = now.with_timezone(&Local);
    if same_minute(started, now) {
        return Ok(false);
    }
    Ok(cron_matches(&spec, now))
}

fn cron_matches(spec: &CronSpec, candidate: DateTime<Local>) -> bool {
    let weekday = candidate.weekday().num_days_from_sunday();
    let day_matches = spec.days.contains(&candidate.day());
    let weekday_matches =
        spec.weekdays.contains(&weekday) || (weekday == 0 && spec.weekdays.contains(&7));
    let calendar_day_matches = if spec.day_is_wildcard && spec.weekday_is_wildcard {
        true
    } else if spec.day_is_wildcard {
        weekday_matches
    } else if spec.weekday_is_wildcard {
        day_matches
    } else {
        day_matches || weekday_matches
    };

    spec.minutes.contains(&candidate.minute())
        && spec.hours.contains(&candidate.hour())
        && spec.months.contains(&candidate.month())
        && calendar_day_matches
}

fn same_minute(left: DateTime<Local>, right: DateTime<Local>) -> bool {
    left.year() == right.year()
        && left.month() == right.month()
        && left.day() == right.day()
        && left.hour() == right.hour()
        && left.minute() == right.minute()
}

fn parse_cron_field(value: &str, min: u32, max: u32) -> PromonResult<Vec<u32>> {
    let mut values = Vec::new();
    for part in value.split(',') {
        if part == "*" {
            values.extend(min..=max);
            continue;
        }
        if let Some(step) = part.strip_prefix("*/") {
            let step: usize = step
                .parse()
                .map_err(|_| PromonError::Config(format!("invalid cron step: {value}")))?;
            if step == 0 {
                return Err(PromonError::Config(format!("invalid cron step: {value}")));
            }
            values.extend((min..=max).step_by(step));
            continue;
        }
        if let Some((start, end)) = part.split_once('-') {
            let start: u32 = start
                .parse()
                .map_err(|_| PromonError::Config(format!("invalid cron range: {value}")))?;
            let end: u32 = end
                .parse()
                .map_err(|_| PromonError::Config(format!("invalid cron range: {value}")))?;
            if start < min || end > max || start > end {
                return Err(PromonError::Config(format!(
                    "cron range out of bounds: {value}"
                )));
            }
            values.extend(start..=end);
            continue;
        }
        let item: u32 = part
            .parse()
            .map_err(|_| PromonError::Config(format!("invalid cron field: {value}")))?;
        if item < min || item > max {
            return Err(PromonError::Config(format!(
                "cron field out of bounds: {value}"
            )));
        }
        values.push(item);
    }
    values.sort_unstable();
    values.dedup();
    Ok(values)
}

fn process_memory_bytes(pid: u32) -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_processes(
        sysinfo::ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]),
        true,
    );
    system
        .process(sysinfo::Pid::from_u32(pid))
        .map(|process| process.memory())
        .unwrap_or(0)
}

pub async fn stop_app(name: &str) -> PromonResult<Option<ManagedProcess>> {
    let Some(process) = remove_process(name).await? else {
        return Ok(None);
    };

    if is_managed_process_alive(&process) {
        terminate_process_tree(process.pid)
            .await
            .map_err(PromonError::Io)?;
        sleep(Duration::from_millis(700)).await;
        if is_managed_process_alive(&process) {
            force_kill_process_tree(process.pid)
                .await
                .map_err(PromonError::Io)?;
        }
    }

    Ok(Some(process))
}

pub async fn stop_all() -> PromonResult<Vec<ManagedProcess>> {
    let processes = load_processes().await?;
    save_processes(&[]).await?;
    for process in &processes {
        if is_managed_process_alive(process) {
            terminate_process_tree(process.pid)
                .await
                .map_err(PromonError::Io)?;
            sleep(Duration::from_millis(700)).await;
            if is_managed_process_alive(process) {
                force_kill_process_tree(process.pid)
                    .await
                    .map_err(PromonError::Io)?;
            }
        }
    }
    Ok(processes)
}

fn configure_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        command.process_group(0);
    }
}

fn is_managed_process_alive(process: &ManagedProcess) -> bool {
    if !is_process_alive(process.pid) {
        return false;
    }
    let Some(command) = process_command(process.pid) else {
        return true;
    };
    let program = process
        .command
        .program
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let matches_program = command.contains(program);
    let matches_arg = process
        .command
        .args
        .first()
        .map(|arg| command.contains(arg))
        .unwrap_or(true);
    matches_program && matches_arg
}

pub async fn restart_app(app: &ResolvedAppSpec) -> PromonResult<ManagedProcess> {
    let _ = stop_app(&app.name).await?;
    start_app(app).await
}

pub async fn list_apps() -> PromonResult<Vec<ManagedProcess>> {
    let mut processes = load_processes().await?;
    for process in &mut processes {
        process.status = if is_managed_process_alive(process) {
            ProcessStatus::Running
        } else {
            ProcessStatus::Unknown
        };
    }
    Ok(processes)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use promon_core::{ExecMode, Instances, LogPolicy, RestartPolicy, RuntimeCommand, WatchSpec};

    use super::*;

    #[test]
    fn parses_memory_units() {
        assert_eq!(parse_memory_limit("64M").unwrap(), 64 * 1024 * 1024);
        assert_eq!(parse_memory_limit("1G").unwrap(), 1024 * 1024 * 1024);
    }

    #[test]
    fn parses_interval_restart() {
        assert_eq!(parse_restart_delay("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_restart_delay("5m").unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn parses_cron_fields() {
        assert_eq!(
            parse_cron_field("*/15", 0, 59).unwrap(),
            vec![0, 15, 30, 45]
        );
        assert_eq!(parse_cron_field("1,3-5", 0, 7).unwrap(), vec![1, 3, 4, 5]);
    }

    #[test]
    fn interval_policy_restarts_after_elapsed_duration() {
        let mut app = test_app();
        app.cron_restart = Some("2s".to_string());
        let mut process = test_process();
        process.started_at = Utc::now() - chrono::Duration::seconds(3);

        let reason = policy_restart_reason_at(&app, &process, Utc::now(), 0)
            .unwrap()
            .unwrap();
        assert_eq!(
            reason,
            PolicyRestartReason::Scheduled {
                rule: "2s".to_string()
            }
        );
    }

    #[test]
    fn memory_policy_restarts_when_limit_is_exceeded() {
        let mut app = test_app();
        app.max_memory_restart = Some("64M".to_string());
        let process = test_process();

        let reason = policy_restart_reason_at(&app, &process, Utc::now(), 65 * 1024 * 1024)
            .unwrap()
            .unwrap();
        assert_eq!(
            reason,
            PolicyRestartReason::MemoryLimit {
                used_bytes: 65 * 1024 * 1024,
                limit_bytes: 64 * 1024 * 1024
            }
        );
    }

    #[test]
    fn cron_policy_matches_current_minute_only_for_older_processes() {
        let now = Utc::now();
        let local_now = now.with_timezone(&Local);
        let mut app = test_app();
        app.cron_restart = Some(format!("{} {} * * *", local_now.minute(), local_now.hour()));

        let mut old_process = test_process();
        old_process.started_at = now - chrono::Duration::minutes(2);
        assert!(policy_restart_reason_at(&app, &old_process, now, 0)
            .unwrap()
            .is_some());

        let mut new_process = test_process();
        new_process.started_at = now;
        assert!(policy_restart_reason_at(&app, &new_process, now, 0)
            .unwrap()
            .is_none());
    }

    fn test_app() -> ResolvedAppSpec {
        ResolvedAppSpec {
            name: "api".to_string(),
            script: Some(PathBuf::from("server.js")),
            command: None,
            cwd: PathBuf::from("/tmp/api"),
            args: vec![],
            node_args: vec![],
            interpreter: "node".to_string(),
            interpreter_args: vec![],
            package_manager: None,
            package_script: None,
            env: BTreeMap::new(),
            exec_mode: ExecMode::Fork,
            instances: Instances::Count(1),
            watch: WatchSpec::default(),
            restart: RestartPolicy::default(),
            max_memory_restart: None,
            cron_restart: None,
            log: LogPolicy::default(),
        }
    }

    fn test_process() -> ManagedProcess {
        ManagedProcess {
            name: "api".to_string(),
            pid: 123,
            status: ProcessStatus::Running,
            cwd: PathBuf::from("/tmp/api"),
            command: RuntimeCommand {
                program: PathBuf::from("node"),
                args: vec!["server.js".to_string()],
                cwd: PathBuf::from("/tmp/api"),
                env: BTreeMap::new(),
            },
            started_at: Utc::now(),
            out_log: PathBuf::from("/tmp/api/out.log"),
            err_log: PathBuf::from("/tmp/api/err.log"),
        }
    }
}
