//! Git-native experiment version management.
//!
//! Each successful experiment is recorded as a commit on an
//! `experiment/{tag}` branch.  Failed experiments are discarded by
//! resetting back to HEAD.  Commit messages follow the format
//! `experiment({run_id}): {description}\n\nMetrics: {json}` so that
//! history is searchable via `git log --grep`.

use std::path::Path;

use anyhow::Context;
use git2::{DiffOptions, IndexAddOption, Repository, ResetType, StatusOptions};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single entry extracted from `git log` for experiment commits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentLogEntry {
    pub hash: String,
    pub run_id: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Manages experiment branches and commits via libgit2.
pub struct ExperimentGitManager {
    repo: Repository,
    original_branch: String,
}

impl ExperimentGitManager {
    // -- Construction -------------------------------------------------------

    /// Open the repository at `repo_dir` and record the current branch.
    pub fn new(repo_dir: &Path) -> anyhow::Result<Self> {
        let repo = Repository::open(repo_dir)
            .with_context(|| format!("failed to open git repo at {}", repo_dir.display()))?;

        let original_branch = current_branch_name(&repo)?;

        Ok(Self {
            repo,
            original_branch,
        })
    }

    /// Returns `true` when `path` is inside a Git repository.
    pub fn is_git_repo(path: &Path) -> bool {
        Repository::open(path).is_ok()
    }

    // -- Branch management --------------------------------------------------

    /// Create and check out `experiment/{tag}`.
    ///
    /// Returns the full branch name.
    pub fn create_experiment_branch(&self, tag: &str) -> anyhow::Result<String> {
        let branch_name = format!("experiment/{tag}");

        // Resolve HEAD commit to use as the branch base.
        let head_commit = self
            .repo
            .head()?
            .peel_to_commit()
            .context("HEAD does not point to a commit")?;

        // Create the branch (force = false).
        self.repo
            .branch(&branch_name, &head_commit, false)
            .with_context(|| format!("failed to create branch {branch_name}"))?;

        // Check it out.
        let refname = format!("refs/heads/{branch_name}");
        let obj = self
            .repo
            .revparse_single(&refname)
            .context("cannot resolve new branch")?;
        self.repo.checkout_tree(&obj, None)?;
        self.repo.set_head(&refname)?;

        Ok(branch_name)
    }

    /// Return the name of the currently checked-out branch.
    pub fn get_current_branch(&self) -> anyhow::Result<String> {
        current_branch_name(&self.repo)
    }

    /// Switch back to the branch that was active when `new()` was called.
    pub fn return_to_original_branch(&self) -> anyhow::Result<()> {
        let refname = format!("refs/heads/{}", self.original_branch);
        let obj = self
            .repo
            .revparse_single(&refname)
            .context("cannot resolve original branch")?;
        self.repo.checkout_tree(&obj, None)?;
        self.repo.set_head(&refname)?;
        Ok(())
    }

    // -- Commit / discard ---------------------------------------------------

    /// Stage all changes (including untracked files) and commit.
    ///
    /// The commit message follows the canonical experiment format:
    /// ```text
    /// experiment({run_id}): {description}
    ///
    /// Metrics: {json}
    /// ```
    ///
    /// Returns the full hex hash of the new commit.
    pub fn commit_experiment(
        &self,
        run_id: &str,
        metrics: &serde_json::Value,
        description: &str,
    ) -> anyhow::Result<String> {
        // Stage everything (git add -A equivalent).
        let mut index = self.repo.index()?;
        index.add_all(["*"], IndexAddOption::DEFAULT, None)?;
        // Also remove deleted files.
        index.update_all(["*"], None)?;
        index.write()?;

        let tree_oid = index.write_tree()?;
        let tree = self.repo.find_tree(tree_oid)?;

        let sig = self
            .repo
            .signature()
            .unwrap_or_else(|_| git2::Signature::now("mol-experiment", "mol@experiment").unwrap());

        let message = format!(
            "experiment({run_id}): {description}\n\nMetrics: {}",
            serde_json::to_string(metrics).unwrap_or_default()
        );

        let parent = self.repo.head()?.peel_to_commit()?;
        let oid = self
            .repo
            .commit(Some("HEAD"), &sig, &sig, &message, &tree, &[&parent])?;

        Ok(oid.to_string())
    }

    /// Discard the current experiment by hard-resetting to HEAD.
    pub fn discard_experiment(&self) -> anyhow::Result<()> {
        let head = self.repo.head()?.peel_to_commit()?;
        let obj = head.into_object();
        self.repo
            .reset(&obj, ResetType::Hard, None)
            .context("git reset --hard HEAD failed")?;
        Ok(())
    }

    // -- History ------------------------------------------------------------

    /// Walk the log of the current branch and return all commits whose
    /// message starts with `experiment(`.
    pub fn get_experiment_history(&self) -> anyhow::Result<Vec<ExperimentLogEntry>> {
        let mut revwalk = self.repo.revwalk()?;
        revwalk.push_head()?;
        revwalk.set_sorting(git2::Sort::TIME)?;

        let mut entries = Vec::new();

        for oid in revwalk {
            let oid = oid?;
            let commit = self.repo.find_commit(oid)?;
            let message = commit.message().unwrap_or_default().to_string();

            if let Some(run_id) = parse_experiment_run_id(&message) {
                entries.push(ExperimentLogEntry {
                    hash: oid.to_string(),
                    run_id,
                    message,
                });
            }
        }

        Ok(entries)
    }

    // -- Diff / clean -------------------------------------------------------

    /// Return `git diff --stat` between HEAD and the working tree.
    pub fn get_experiment_diff(&self) -> anyhow::Result<String> {
        let head_tree = self.repo.head()?.peel_to_tree()?;

        let diff = self.repo.diff_tree_to_workdir_with_index(
            Some(&head_tree),
            Some(DiffOptions::new().include_untracked(true)),
        )?;

        let stats = diff.stats()?;
        let buf = stats.to_buf(git2::DiffStatsFormat::FULL, 80)?;
        Ok(buf.as_str().unwrap_or_default().to_string())
    }

    /// Remove untracked files and directories (`git clean -fd`).
    pub fn clean_untracked(&self) -> anyhow::Result<()> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true);
        opts.recurse_untracked_dirs(true);

        let statuses = self.repo.statuses(Some(&mut opts))?;

        for entry in statuses.iter() {
            let status = entry.status();
            if status.contains(git2::Status::WT_NEW) {
                if let Some(path) = entry.path() {
                    let full = self.repo.workdir().unwrap().join(path);
                    if full.is_dir() {
                        std::fs::remove_dir_all(&full).ok();
                    } else {
                        std::fs::remove_file(&full).ok();
                    }
                }
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn current_branch_name(repo: &Repository) -> anyhow::Result<String> {
    let head = repo.head().context("no HEAD – is the repository empty?")?;
    if head.is_branch() {
        Ok(head
            .shorthand()
            .unwrap_or("HEAD")
            .to_string())
    } else {
        // Detached HEAD – fall back to the OID.
        Ok(head
            .target()
            .map(|o| o.to_string())
            .unwrap_or_else(|| "HEAD".into()))
    }
}

/// Extract the `run_id` from an `experiment(run_id): ...` message.
fn parse_experiment_run_id(message: &str) -> Option<String> {
    let s = message.strip_prefix("experiment(")?;
    let end = s.find(')')?;
    Some(s[..end].to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Create a temporary git repo with an initial (empty-tree) commit so
    /// that HEAD exists and branches can be created.
    fn init_test_repo() -> (tempfile::TempDir, ExperimentGitManager) {
        let tmp = tempdir().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();

        let sig = repo
            .signature()
            .unwrap_or_else(|_| git2::Signature::now("test", "test@test.com").unwrap());

        let tree_id = repo.index().unwrap().write_tree().unwrap();
        {
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
                .unwrap();
        }

        drop(repo);

        let mgr = ExperimentGitManager::new(tmp.path()).unwrap();
        (tmp, mgr)
    }

    #[test]
    fn is_git_repo_true_for_repo() {
        let (tmp, _) = init_test_repo();
        assert!(ExperimentGitManager::is_git_repo(tmp.path()));
    }

    #[test]
    fn is_git_repo_false_for_non_repo() {
        let tmp = tempdir().unwrap();
        assert!(!ExperimentGitManager::is_git_repo(tmp.path()));
    }

    #[test]
    fn create_experiment_branch() {
        let (_tmp, mgr) = init_test_repo();
        let branch = mgr.create_experiment_branch("test-run").unwrap();
        assert_eq!(branch, "experiment/test-run");
        assert_eq!(mgr.get_current_branch().unwrap(), "experiment/test-run");
    }

    #[test]
    fn commit_and_history() {
        let (tmp, mgr) = init_test_repo();
        mgr.create_experiment_branch("run1").unwrap();

        // Write a file so there is something to commit.
        std::fs::write(tmp.path().join("result.txt"), "data").unwrap();

        let metrics = serde_json::json!({"accuracy": 0.95});
        let hash = mgr
            .commit_experiment("run1", &metrics, "first experiment")
            .unwrap();
        assert!(!hash.is_empty());

        let history = mgr.get_experiment_history().unwrap();
        assert!(!history.is_empty());
        assert!(history.iter().any(|e| e.run_id == "run1"));
    }

    #[test]
    fn discard_experiment_resets_changes() {
        let (tmp, mgr) = init_test_repo();
        mgr.create_experiment_branch("discard-run").unwrap();

        let file = tmp.path().join("scratch.txt");
        std::fs::write(&file, "temporary").unwrap();

        // Stage the file so it's tracked, then discard.
        {
            let mut idx = mgr.repo.index().unwrap();
            idx.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
            idx.write().unwrap();
        }

        mgr.discard_experiment().unwrap();

        // The file itself is untracked (reset to HEAD which didn't have it),
        // but the index should be clean.
        let head_tree = mgr.repo.head().unwrap().peel_to_tree().unwrap();
        let diff = mgr
            .repo
            .diff_tree_to_index(Some(&head_tree), None, None)
            .unwrap();
        assert_eq!(diff.deltas().count(), 0);
    }

    #[test]
    fn return_to_original_branch() {
        let (_tmp, mgr) = init_test_repo();
        let original = mgr.get_current_branch().unwrap();

        mgr.create_experiment_branch("detour").unwrap();
        assert_eq!(mgr.get_current_branch().unwrap(), "experiment/detour");

        mgr.return_to_original_branch().unwrap();
        assert_eq!(mgr.get_current_branch().unwrap(), original);
    }

    #[test]
    fn get_experiment_diff_reports_changes() {
        let (tmp, mgr) = init_test_repo();
        std::fs::write(tmp.path().join("new_file.txt"), "hello").unwrap();

        let diff = mgr.get_experiment_diff().unwrap();
        assert!(
            diff.contains("1 file changed") || diff.contains("new_file"),
            "diff should report the new file, got: {diff}"
        );
    }

    #[test]
    fn clean_untracked_removes_files() {
        let (tmp, mgr) = init_test_repo();
        let untracked = tmp.path().join("junk.txt");
        std::fs::write(&untracked, "junk").unwrap();
        assert!(untracked.exists());

        mgr.clean_untracked().unwrap();
        assert!(!untracked.exists());
    }

    #[test]
    fn parse_experiment_run_id_works() {
        assert_eq!(
            parse_experiment_run_id("experiment(run42): stuff"),
            Some("run42".to_string())
        );
        assert_eq!(parse_experiment_run_id("not an experiment"), None);
    }
}
