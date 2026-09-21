//! Orchestrates one extraction job inside the `docs/SANDBOX.md` boundary.
//!
//! This crate never opens, parses, decompresses, executes, or previews
//! artifact bytes itself — it writes them to a per-job input directory,
//! launches a hardened, disposable Docker container against that directory,
//! and reads back a typed, size-bounded result file the container wrote.
//! The extraction logic lives entirely inside the container image; this
//! process only ever sees the container's declared stdout/stderr and the
//! bytes of its result file.
//!
//! Isolation mechanism: Docker, invoked per job with `--network none`,
//! `--read-only` root filesystem, `--cap-drop ALL`,
//! `--security-opt no-new-privileges`, an unprivileged numeric user, and
//! explicit memory/CPU/pids/wall-clock limits. No application database
//! credentials, object-store write capability, shell, or inherited host
//! environment variable reaches the container: this process passes no `-e`
//! flags at all, so the container's environment is whatever its image sets,
//! nothing this process's environment carries.

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct SandboxLimits {
    pub memory_bytes: u64,
    pub cpus: f64,
    pub pids_limit: u32,
    pub wall_clock: Duration,
    /// Cap on the result file this process will read back. A container
    /// that writes more than this is treated the same as one that produced
    /// no usable result: this process never allocates unboundedly for
    /// output any more than for input.
    pub max_output_bytes: u64,
}

impl SandboxLimits {
    pub const fn conservative_default() -> Self {
        Self {
            memory_bytes: 256 * 1024 * 1024,
            cpus: 0.5,
            pids_limit: 32,
            wall_clock: Duration::from_secs(10),
            max_output_bytes: 4 * 1024 * 1024,
        }
    }
}

#[derive(Debug)]
pub enum SandboxError {
    Io(std::io::Error),
    DockerLaunchFailed(String),
    /// The container ran past `wall_clock` and was killed.
    TimedOut,
    /// The container exited non-zero, or was killed for a reason other
    /// than this process's own timeout (e.g. the Docker daemon's own
    /// resource enforcement, or the extractor's own unhandled failure).
    ExtractorCrashed {
        exit_code: Option<i32>,
        stderr: String,
    },
    NoOutputProduced,
    OutputTooLarge,
}

impl From<std::io::Error> for SandboxError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// Runs `image` against `input_bytes` and returns the raw bytes of
/// `/output/result.json` as the container wrote them. This function does
/// not interpret those bytes; the caller (an extractor-specific adapter)
/// is responsible for parsing the typed shape a given extractor image
/// promises to produce.
pub fn run_extraction(
    image: &str,
    input_bytes: &[u8],
    limits: &SandboxLimits,
) -> Result<Vec<u8>, SandboxError> {
    let job = JobDir::create()?;
    job.write_input(input_bytes)?;

    let mut command = Command::new("docker");
    command
        .arg("run")
        .arg("--rm")
        .arg("--name")
        .arg(&job.id)
        .arg("--network")
        .arg("none")
        .arg("--read-only")
        .arg("--tmpfs")
        .arg("/tmp:rw,size=16m,noexec,nosuid")
        .arg("--memory")
        .arg(limits.memory_bytes.to_string())
        .arg("--memory-swap")
        .arg(limits.memory_bytes.to_string())
        .arg("--cpus")
        .arg(format!("{}", limits.cpus))
        .arg("--pids-limit")
        .arg(limits.pids_limit.to_string())
        .arg("--cap-drop")
        .arg("ALL")
        .arg("--security-opt")
        .arg("no-new-privileges")
        .arg("--user")
        .arg("65534:65534")
        .arg("-v")
        .arg(format!("{}:/input:ro", job.input_dir().display()))
        .arg("-v")
        .arg(format!("{}:/output:rw", job.output_dir().display()))
        .arg(image)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = command
        .spawn()
        .map_err(|e| SandboxError::DockerLaunchFailed(e.to_string()))?;

    let timed_out = Arc::new(AtomicBool::new(false));
    let watcher = {
        let timed_out = Arc::clone(&timed_out);
        let name = job.id.clone();
        let deadline = limits.wall_clock;
        std::thread::spawn(move || {
            std::thread::sleep(deadline);
            timed_out.store(true, Ordering::SeqCst);
            // Best effort: if the container already exited this simply
            // fails (nothing named `name` left to kill), which is fine.
            let _ = Command::new("docker")
                .arg("kill")
                .arg(&name)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        })
    };

    let output = child.wait_with_output()?;
    // The container has exited one way or another; stop caring whether the
    // watcher still fires (it is harmless if it does, `docker kill` on an
    // already-removed `--rm` container just errors silently).
    drop(watcher);

    if timed_out.load(Ordering::SeqCst) {
        return Err(SandboxError::TimedOut);
    }
    if !output.status.success() {
        return Err(SandboxError::ExtractorCrashed {
            exit_code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    job.read_result(limits.max_output_bytes)
}

struct JobDir {
    root: PathBuf,
    id: String,
}

impl JobDir {
    fn create() -> Result<Self, SandboxError> {
        let id = format!(
            "nexo-sandbox-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(&id);
        fs::create_dir_all(root.join("input"))?;
        fs::create_dir_all(root.join("output"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // The container runs as an unprivileged, non-host user
            // (65534:65534) with no user-namespace remapping, so a bind
            // mount keeps host ownership: without world-write here the
            // container user cannot create its result file at all.
            fs::set_permissions(root.join("output"), fs::Permissions::from_mode(0o777))?;
        }
        Ok(Self { root, id })
    }

    fn input_dir(&self) -> PathBuf {
        self.root.join("input")
    }

    fn output_dir(&self) -> PathBuf {
        self.root.join("output")
    }

    fn write_input(&self, bytes: &[u8]) -> Result<(), SandboxError> {
        let path = self.input_dir().join("artifact");
        fs::write(&path, bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Read-only for everyone (including the container's
            // unprivileged, non-host uid 65534) once written: the
            // container also mounts this directory `:ro`, but the
            // host-side permission is a second, independent guard against
            // this process's own code accidentally mutating the job's
            // input after the container has started reading it.
            fs::set_permissions(&path, fs::Permissions::from_mode(0o444))?;
        }
        Ok(())
    }

    fn read_result(&self, max_output_bytes: u64) -> Result<Vec<u8>, SandboxError> {
        let path = self.output_dir().join("result.json");
        let mut file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(SandboxError::NoOutputProduced);
            }
            Err(err) => return Err(err.into()),
        };
        let mut buffer = Vec::new();
        let mut limited = file.by_ref().take(max_output_bytes + 1);
        limited.read_to_end(&mut buffer)?;
        if buffer.len() as u64 > max_output_bytes {
            return Err(SandboxError::OutputTooLarge);
        }
        Ok(buffer)
    }
}

impl Drop for JobDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docker_available() -> bool {
        Command::new("docker")
            .arg("info")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    /// `run_extraction` does not expose a custom command (by design — an
    /// extractor image defines its own `ENTRYPOINT`/`CMD`), so this is a
    /// harness smoke test: a plain `alpine:3` image with no matching
    /// entrypoint behavior must surface as a typed `SandboxError`, never a
    /// panic or a hang. `sandbox_flags_block_network_and_root_filesystem_write`
    /// below is the actual assertion about the isolation flags themselves.
    #[test]
    fn run_extraction_surfaces_a_typed_error_for_an_image_producing_no_result() {
        if !docker_available() {
            eprintln!("skipping: docker not available in this environment");
            return;
        }
        let limits = SandboxLimits {
            wall_clock: Duration::from_secs(15),
            ..SandboxLimits::conservative_default()
        };
        let result = run_extraction("alpine:3", b"unused", &limits);
        assert!(matches!(
            result,
            Err(SandboxError::ExtractorCrashed { .. }) | Err(SandboxError::NoOutputProduced)
        ));
    }

    #[test]
    fn sandbox_flags_block_network_and_root_filesystem_write() {
        if !docker_available() {
            eprintln!("skipping: docker not available in this environment");
            return;
        }
        let network_probe = Command::new("docker")
            .args([
                "run", "--rm", "--network", "none", "--read-only", "--cap-drop", "ALL",
                "--security-opt", "no-new-privileges", "--user", "65534:65534", "alpine:3",
                "sh", "-c", "wget -T 2 -O /dev/null http://example.com",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(
            !network_probe.success(),
            "network reachability must be blocked by --network none"
        );

        let root_write_probe = Command::new("docker")
            .args([
                "run", "--rm", "--network", "none", "--read-only", "--cap-drop", "ALL",
                "--security-opt", "no-new-privileges", "--user", "65534:65534", "alpine:3",
                "sh", "-c", "touch /root-write-test",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(
            !root_write_probe.success(),
            "root filesystem must be read-only"
        );
    }

    fn hostile_sleep_image_available() -> bool {
        Command::new("docker")
            .args(["image", "inspect", "nexo-sandbox-test-sleep:local"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    /// Proves the wall-clock enforcement actually kills a runaway
    /// container rather than hanging this process forever. Built from
    /// `testdata/hostile-sleep/Dockerfile`:
    /// `docker build -t nexo-sandbox-test-sleep:local testdata/hostile-sleep`.
    #[test]
    fn wall_clock_timeout_kills_a_runaway_container() {
        if !docker_available() || !hostile_sleep_image_available() {
            eprintln!(
                "skipping: docker not available or nexo-sandbox-test-sleep:local not built \
                 (docker build -t nexo-sandbox-test-sleep:local testdata/hostile-sleep)"
            );
            return;
        }
        let limits = SandboxLimits {
            wall_clock: Duration::from_secs(2),
            ..SandboxLimits::conservative_default()
        };
        let started = std::time::Instant::now();
        let result = run_extraction("nexo-sandbox-test-sleep:local", b"unused", &limits);
        let elapsed = started.elapsed();
        assert!(matches!(result, Err(SandboxError::TimedOut)));
        assert!(
            elapsed < Duration::from_secs(6),
            "kill should land well before 6s for a 2s deadline, took {elapsed:?}"
        );
    }
}
