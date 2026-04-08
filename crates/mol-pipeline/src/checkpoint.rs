//! Checkpoint and heartbeat I/O for the pipeline runner.
//!
//! Checkpoints are written atomically via a tempfile + rename pattern to
//! prevent corruption if the process is killed mid-write.  This mirrors the
//! Python `_write_checkpoint` / `read_checkpoint` helpers in `runner.py`.

use crate::stages::{Stage, StageStatus, STAGE_SEQUENCE};
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// On-disk checkpoint format
// ---------------------------------------------------------------------------

/// JSON structure written to `checkpoint.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CheckpointRecord {
    /// Numeric value of the last completed stage.
    last_completed_stage: i32,
    /// Human-readable name of the last completed stage.
    last_completed_name: String,
    /// Run identifier.
    run_id: String,
    /// ISO-8601 timestamp.
    timestamp: String,
}

/// JSON structure written to `heartbeat.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HeartbeatRecord {
    pid: u32,
    last_stage: i32,
    last_stage_name: String,
    run_id: String,
    timestamp: String,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Write a checkpoint atomically via tempfile + rename.
///
/// The file is always `{run_dir}/checkpoint.json`.  On success the previous
/// checkpoint (if any) is replaced atomically.
pub async fn write_checkpoint(
    run_dir: &Path,
    stage: Stage,
    run_id: &str,
    status: StageStatus,
) -> Result<()> {
    let record = CheckpointRecord {
        last_completed_stage: stage.as_i32(),
        last_completed_name: stage.name().to_owned(),
        run_id: run_id.to_owned(),
        timestamp: Utc::now().to_rfc3339(),
    };
    let json = serde_json::to_string_pretty(&record).context("serialise checkpoint")?;

    let target = run_dir.join("checkpoint.json");
    atomic_write(&target, json.as_bytes()).await.with_context(|| {
        format!(
            "write checkpoint for stage {} (status={}) to {}",
            stage.name(),
            status,
            target.display()
        )
    })?;

    debug!(
        stage = stage.name(),
        status = %status,
        path = %target.display(),
        "checkpoint written"
    );
    Ok(())
}

/// Read the checkpoint file and return the raw `(Stage, StageStatus)` pair,
/// or `None` if no checkpoint exists or the file is unreadable / corrupt.
///
/// The returned `StageStatus` is always `Done` — checkpoints record completed
/// stages only.
pub async fn read_checkpoint(run_dir: &Path) -> Result<Option<(Stage, StageStatus)>> {
    let cp_path = run_dir.join("checkpoint.json");
    if !cp_path.exists() {
        return Ok(None);
    }

    let text = match fs::read_to_string(&cp_path).await {
        Ok(t) => t,
        Err(e) => {
            warn!(path = %cp_path.display(), error = %e, "failed to read checkpoint file");
            return Ok(None);
        }
    };

    let record: CheckpointRecord = match serde_json::from_str(&text) {
        Ok(r) => r,
        Err(e) => {
            warn!(path = %cp_path.display(), error = %e, "checkpoint JSON is corrupt");
            return Ok(None);
        }
    };

    match Stage::try_from(record.last_completed_stage) {
        Ok(stage) => Ok(Some((stage, StageStatus::Done))),
        Err(_) => {
            warn!(
                value = record.last_completed_stage,
                "unknown stage number in checkpoint"
            );
            Ok(None)
        }
    }
}

/// Resolve the stage from which a run should resume.
///
/// If a checkpoint exists this is the stage *after* the last completed one.
/// Falls back to `default_stage` (typically `Stage::TopicInit`) when no
/// checkpoint is found.
pub fn resume_from_checkpoint(checkpoint: (Stage, StageStatus)) -> Stage {
    let (last_completed, _status) = checkpoint;
    // Find the next stage in the canonical sequence
    STAGE_SEQUENCE
        .iter()
        .position(|&s| s == last_completed)
        .and_then(|i| STAGE_SEQUENCE.get(i + 1))
        .copied()
        .unwrap_or(last_completed)
}

/// Write a heartbeat file for sentinel watchdog monitoring.
///
/// The heartbeat is written in-place (not atomically) as it is advisory only.
pub async fn write_heartbeat(run_dir: &Path, stage: Stage, run_id: &str) -> Result<()> {
    let record = HeartbeatRecord {
        pid: std::process::id(),
        last_stage: stage.as_i32(),
        last_stage_name: stage.name().to_owned(),
        run_id: run_id.to_owned(),
        timestamp: Utc::now().to_rfc3339(),
    };
    let json = serde_json::to_string_pretty(&record).context("serialise heartbeat")?;
    let path = run_dir.join("heartbeat.json");
    fs::write(&path, json.as_bytes())
        .await
        .with_context(|| format!("write heartbeat to {}", path.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Atomic write helper
// ---------------------------------------------------------------------------

/// Write `data` to `target_path` via a sibling tempfile + rename, ensuring
/// that partial writes never corrupt the target.
async fn atomic_write(target_path: &Path, data: &[u8]) -> Result<()> {
    let parent = target_path
        .parent()
        .context("checkpoint path has no parent directory")?;

    // Create a named tempfile in the same directory so that rename is atomic
    // (both paths are on the same filesystem).
    let tmp_path = {
        // Use sync tempfile::Builder for the path, then hand off to tokio.
        let tmp = tempfile::Builder::new()
            .prefix("checkpoint_")
            .suffix(".tmp")
            .tempfile_in(parent)
            .context("create tempfile for atomic write")?;
        // Persist the tempfile (drop the File handle, keep the PathBuf).
        let (_file, path) = tmp.keep().context("persist tempfile")?;
        path
    };

    // Write data to tempfile.
    if let Err(e) = fs::write(&tmp_path, data).await {
        // Best-effort cleanup.
        let _ = fs::remove_file(&tmp_path).await;
        return Err(e).context("write data to tempfile");
    }

    // Atomically rename to target.
    if let Err(e) = fs::rename(&tmp_path, target_path).await {
        let _ = fs::remove_file(&tmp_path).await;
        return Err(e).context("rename tempfile to target");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn roundtrip_checkpoint() {
        let dir = TempDir::new().unwrap();
        let run_dir = dir.path();

        // No checkpoint yet.
        assert!(read_checkpoint(run_dir).await.unwrap().is_none());

        write_checkpoint(run_dir, Stage::HypothesisGen, "run-001", StageStatus::Done)
            .await
            .unwrap();

        let cp = read_checkpoint(run_dir).await.unwrap().unwrap();
        assert_eq!(cp.0, Stage::HypothesisGen);
        assert_eq!(cp.1, StageStatus::Done);
    }

    #[tokio::test]
    async fn resume_advances_one_step() {
        let cp = (Stage::HypothesisGen, StageStatus::Done);
        let next = resume_from_checkpoint(cp);
        assert_eq!(next, Stage::ExperimentDesign);
    }

    #[tokio::test]
    async fn resume_from_last_stage_returns_last() {
        let cp = (Stage::CitationVerify, StageStatus::Done);
        let next = resume_from_checkpoint(cp);
        // No stage after CitationVerify → returns same stage
        assert_eq!(next, Stage::CitationVerify);
    }

    #[tokio::test]
    async fn write_heartbeat_creates_file() {
        let dir = TempDir::new().unwrap();
        write_heartbeat(dir.path(), Stage::ExperimentRun, "run-hb")
            .await
            .unwrap();
        assert!(dir.path().join("heartbeat.json").exists());
    }

    #[tokio::test]
    async fn corrupt_checkpoint_returns_none() {
        let dir = TempDir::new().unwrap();
        tokio::fs::write(dir.path().join("checkpoint.json"), b"not json")
            .await
            .unwrap();
        let result = read_checkpoint(dir.path()).await.unwrap();
        assert!(result.is_none());
    }
}
