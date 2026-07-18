use std::env;
use std::fmt::Display;
use std::fmt::Formatter;
use std::path::Path;
use std::path::PathBuf;

const NODE_OVERRIDE_ENV: &str = "DPRINT_OXFMT_NODE";
const WORKER_OVERRIDE_ENV: &str = "DPRINT_OXFMT_WORKER";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeLocator {
    node_program: PathBuf,
    worker_entry: PathBuf,
}

impl RuntimeLocator {
    #[must_use]
    pub const fn new(node_program: PathBuf, worker_entry: PathBuf) -> Self {
        Self {
            node_program,
            worker_entry,
        }
    }

    /// Discovers the Node executable and packaged worker entry point.
    ///
    /// # Errors
    ///
    /// Returns an error when no worker entry point exists in the configured or
    /// default locations.
    pub fn discover() -> Result<Self, RuntimeLocatorError> {
        let node_program =
            env::var_os(NODE_OVERRIDE_ENV).map_or_else(|| PathBuf::from("node"), PathBuf::from);
        let candidates = worker_candidates();

        candidates
            .iter()
            .find(|candidate| candidate.is_file())
            .cloned()
            .map(|worker_entry| Self::new(node_program.clone(), worker_entry))
            .ok_or(RuntimeLocatorError {
                node_program,
                candidates,
            })
    }

    #[must_use]
    pub fn node_program(&self) -> &Path {
        self.node_program.as_path()
    }

    #[must_use]
    pub fn worker_entry(&self) -> &Path {
        self.worker_entry.as_path()
    }
}

#[derive(Debug)]
pub struct RuntimeLocatorError {
    node_program: PathBuf,
    candidates: Vec<PathBuf>,
}

impl Display for RuntimeLocatorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "could not locate the Oxfmt worker using Node {}; checked: {}",
            self.node_program.display(),
            self.candidates
                .iter()
                .map(|candidate| candidate.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl std::error::Error for RuntimeLocatorError {}

fn worker_candidates() -> Vec<PathBuf> {
    if let Some(worker_override) = env::var_os(WORKER_OVERRIDE_ENV) {
        return vec![PathBuf::from(worker_override)];
    }

    default_worker_candidates(env::current_exe().ok().as_deref())
}

fn default_worker_candidates(executable: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(parent) = executable.and_then(Path::parent) {
        candidates.push(parent.join("runtime/dist/worker.js"));
        candidates.push(parent.join("runtime/worker.js"));
    }
    if cfg!(debug_assertions) {
        candidates
            .push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/dist/worker.js"));
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_explicit_runtime_paths() {
        let locator = RuntimeLocator::new(
            PathBuf::from("custom-node"),
            PathBuf::from("custom-worker.js"),
        );

        assert_eq!(locator.node_program(), Path::new("custom-node"));
        assert_eq!(locator.worker_entry(), Path::new("custom-worker.js"));
    }

    #[test]
    fn packaged_workers_precede_the_development_worker() {
        let executable = Path::new("package/bin/dprint-plugin-oxfmt");
        let candidates = default_worker_candidates(Some(executable));

        assert_eq!(
            candidates.first(),
            Some(&PathBuf::from("package/bin/runtime/dist/worker.js"))
        );
        assert_eq!(
            candidates.get(1),
            Some(&PathBuf::from("package/bin/runtime/worker.js"))
        );
        if cfg!(debug_assertions) {
            assert_eq!(
                candidates.last(),
                Some(
                    &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/dist/worker.js")
                )
            );
        }
    }

    #[test]
    fn reports_all_default_worker_candidates() {
        let error = RuntimeLocator::new(PathBuf::from("node"), PathBuf::from("missing-worker.js"));
        let display = RuntimeLocatorError {
            node_program: error.node_program.clone(),
            candidates: worker_candidates(),
        }
        .to_string();

        assert!(display.contains("runtime/dist/worker.js"));
    }
}
