use std::path::{Path, PathBuf};

use chrono::Utc;
use promon_core::{ManagedProcess, PromonError, PromonResult};
use promon_platform::state_dir;
use tokio::fs;
use tokio::io::AsyncWriteExt;

fn state_file() -> PathBuf {
    state_dir().join("processes.json")
}

pub async fn load_processes() -> PromonResult<Vec<ManagedProcess>> {
    let path = state_file();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path).await.map_err(PromonError::Io)?;
    match serde_json::from_str(&raw) {
        Ok(processes) => Ok(processes),
        Err(_) => {
            backup_corrupt_state(&path, &raw).await?;
            Ok(Vec::new())
        }
    }
}

pub async fn save_processes(processes: &[ManagedProcess]) -> PromonResult<()> {
    let dir = state_dir();
    fs::create_dir_all(&dir).await.map_err(PromonError::Io)?;
    let raw = serde_json::to_string_pretty(processes).map_err(PromonError::Json)?;
    let tmp = dir.join(format!(
        "processes.json.tmp.{}.{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let mut file = fs::File::create(&tmp).await.map_err(PromonError::Io)?;
    file.write_all(raw.as_bytes())
        .await
        .map_err(PromonError::Io)?;
    file.sync_all().await.map_err(PromonError::Io)?;
    drop(file);
    if let Err(error) = fs::rename(&tmp, state_file()).await {
        let _ = fs::remove_file(&tmp).await;
        return Err(PromonError::Io(error));
    }
    sync_parent_dir(&dir);
    Ok(())
}

pub async fn upsert_process(process: ManagedProcess) -> PromonResult<()> {
    let mut processes = load_processes().await?;
    processes.retain(|item| item.name != process.name);
    processes.push(process);
    save_processes(&processes).await
}

pub async fn remove_process(name: &str) -> PromonResult<Option<ManagedProcess>> {
    let mut processes = load_processes().await?;
    let removed = processes
        .iter()
        .position(|item| item.name == name)
        .map(|index| processes.remove(index));
    save_processes(&processes).await?;
    Ok(removed)
}

async fn backup_corrupt_state(path: &Path, raw: &str) -> PromonResult<()> {
    let backup = state_dir().join(format!(
        "processes.corrupt.{}.json",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    match fs::rename(path, &backup).await {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::write(&backup, raw).await.map_err(PromonError::Io)?;
            let _ = fs::remove_file(path).await;
            Ok(())
        }
    }
}

fn sync_parent_dir(dir: &Path) {
    let _ = std::fs::File::open(dir).and_then(|file| file.sync_all());
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::ffi::OsString;

    use promon_core::{ProcessStatus, RuntimeCommand};
    use promon_platform::state_dir;

    use super::*;

    struct PromonHomeGuard {
        previous: Option<OsString>,
        home: PathBuf,
    }

    impl PromonHomeGuard {
        fn install(name: &str) -> Self {
            let previous = std::env::var_os("PROMON_HOME");
            let home = std::env::temp_dir().join(format!(
                "promon-state-store-{name}-{}-{}",
                std::process::id(),
                Utc::now().timestamp_nanos_opt().unwrap_or_default()
            ));
            std::env::set_var("PROMON_HOME", &home);
            Self { previous, home }
        }
    }

    impl Drop for PromonHomeGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var("PROMON_HOME", previous);
            } else {
                std::env::remove_var("PROMON_HOME");
            }
            let _ = std::fs::remove_dir_all(&self.home);
        }
    }

    #[tokio::test]
    async fn saves_atomically_and_recovers_corrupt_state() {
        let _guard = PromonHomeGuard::install("roundtrip-corrupt");
        let process = ManagedProcess {
            name: "app".to_string(),
            pid: 123,
            status: ProcessStatus::Running,
            cwd: PathBuf::from("/tmp/app"),
            command: RuntimeCommand {
                program: PathBuf::from("node"),
                args: vec!["server.js".to_string()],
                cwd: PathBuf::from("/tmp/app"),
                env: BTreeMap::new(),
            },
            started_at: Utc::now(),
            out_log: PathBuf::from("/tmp/app/out.log"),
            err_log: PathBuf::from("/tmp/app/err.log"),
        };

        save_processes(std::slice::from_ref(&process))
            .await
            .expect("state save should succeed");
        assert_eq!(
            load_processes().await.expect("state load should succeed"),
            vec![process]
        );
        let has_temp_files = std::fs::read_dir(state_dir())
            .expect("state dir should exist")
            .any(|entry| {
                entry
                    .expect("state dir entry should be readable")
                    .file_name()
                    .to_string_lossy()
                    .starts_with("processes.json.tmp")
            });
        assert!(!has_temp_files, "atomic save should not leave temp files");

        let state_path = state_file();
        tokio::fs::write(&state_path, "{bad-json")
            .await
            .expect("corrupt state write should succeed");
        assert_eq!(
            load_processes()
                .await
                .expect("corrupt state should recover"),
            Vec::<ManagedProcess>::new()
        );
        assert!(
            !state_path.exists(),
            "corrupt primary state file should be moved away"
        );

        let backups: Vec<_> = std::fs::read_dir(state_dir())
            .expect("state dir should exist")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("processes.corrupt.")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            std::fs::read_to_string(backups[0].path()).expect("backup should be readable"),
            "{bad-json"
        );
    }
}
