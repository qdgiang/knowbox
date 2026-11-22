use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::GitError;

pub(super) fn command(root: &Path) -> Command {
    let mut cmd = Command::new("git");
    let prefix = b"GIT_CONFIG";
    for (key, _) in std::env::vars_os() {
        if key
            .as_encoded_bytes()
            .get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
        {
            cmd.env_remove(key);
        }
    }
    cmd.current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_LITERAL_PATHSPECS", "1")
        .env("LC_ALL", "C");
    #[cfg(test)]
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_ALLOW_PROTOCOL", "file");
    cmd
}

pub(super) struct Output {
    op: &'static str,
    status: ExitStatus,
    stdout: File,
    stderr: File,
}

impl Output {
    pub(super) fn checked(self) -> Result<Vec<u8>, GitError> {
        let file = self.checked_file()?;
        const LIMIT: u64 = 64 * 1024 * 1024;
        let mut data = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut data)?;
        if data.len() as u64 > LIMIT {
            return Err(GitError::Invalid("git output exceeded 64 MiB"));
        }
        Ok(data)
    }

    pub(super) fn checked_file(self) -> Result<File, GitError> {
        if !self.status.success() {
            return Err(self.failure()?);
        }
        let mut file = self.stdout;
        file.rewind()?;
        Ok(file)
    }

    pub(super) fn optional(self) -> Result<Option<Vec<u8>>, GitError> {
        if self.status.code() == Some(1) {
            return Ok(None);
        }
        self.checked().map(Some)
    }

    fn failure(mut self) -> Result<GitError, GitError> {
        self.stderr.rewind()?;
        let mut snippet = Vec::new();
        self.stderr.take(2048).read_to_end(&mut snippet)?;
        Ok(GitError::Failed {
            op: self.op,
            status: self.status,
            detail: String::from_utf8_lossy(&snippet).trim().to_owned(),
        })
    }
}

pub(super) fn run(cmd: &mut Command, op: &'static str) -> Result<Output, GitError> {
    run_with_input(cmd, op, Stdio::null())
}

pub(super) fn run_with_input(
    cmd: &mut Command,
    op: &'static str,
    input: Stdio,
) -> Result<Output, GitError> {
    // Never wait for pipe EOF: a credential helper, hook or transport may inherit the handles.
    let stdout = tempfile::tempfile()?;
    let stderr = tempfile::tempfile()?;
    let mut child = cmd
        .stdin(input)
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?)
        .spawn()
        .map_err(|source| GitError::Spawn { op, source })?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Output {
                op,
                status,
                stdout,
                stderr,
            });
        }
        let left = Duration::from_secs(60).saturating_sub(start.elapsed());
        if left.is_zero() {
            child.kill()?;
            child.wait()?;
            return Err(GitError::Timeout(op));
        }
        thread::sleep(Duration::from_millis(5).min(left));
    }
}
