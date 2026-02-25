//! Process management for SSH shell and exec commands
//!
//! This module handles spawning and managing shell processes for SSH sessions.
//! When a PTY is requested, we use portable-pty to create a real pseudo-terminal
//! which handles line discipline (converting \n to \r\n, etc.)

#[cfg(feature = "ssh")]
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
#[cfg(feature = "ssh")]
use russh::server::Handle;
#[cfg(feature = "ssh")]
use russh::{ChannelId, CryptoVec};
#[cfg(feature = "ssh")]
use std::collections::HashMap;
#[cfg(feature = "ssh")]
use std::io::{Read, Write};
#[cfg(feature = "ssh")]
use std::process::Stdio;
#[cfg(feature = "ssh")]
use std::sync::Arc;
#[cfg(feature = "ssh")]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(feature = "ssh")]
use tokio::process::Command;
#[cfg(feature = "ssh")]
use tokio::sync::{mpsc, Mutex};

/// Shell process wrapper
#[cfg(feature = "ssh")]
pub struct ShellProcess {
    stdin_tx: mpsc::Sender<Vec<u8>>,
}

#[cfg(feature = "ssh")]
impl ShellProcess {
    /// Send data to the process stdin
    pub async fn write(&self, data: &[u8]) -> anyhow::Result<()> {
        self.stdin_tx.send(data.to_vec()).await?;
        Ok(())
    }
}

/// Shell manager that tracks active shell processes
#[cfg(feature = "ssh")]
pub struct ShellManager {
    shells: Mutex<HashMap<u32, ShellProcess>>,
    /// Output receivers for streaming exec commands
    exec_outputs: Mutex<HashMap<u32, mpsc::Receiver<Vec<u8>>>>,
    /// Exit status receivers for streaming exec commands
    exec_exits: Mutex<HashMap<u32, tokio::sync::oneshot::Receiver<u32>>>,
}

/// PTY configuration for shell spawning
#[cfg(feature = "ssh")]
#[derive(Clone, Debug)]
pub struct PtyConfig {
    /// Number of columns
    pub cols: u16,
    /// Number of rows
    pub rows: u16,
    /// Pixel width (optional, can be 0)
    pub pixel_width: u16,
    /// Pixel height (optional, can be 0)
    pub pixel_height: u16,
}

#[cfg(feature = "ssh")]
impl Default for PtyConfig {
    fn default() -> Self {
        Self {
            cols: 80,
            rows: 24,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

#[cfg(feature = "ssh")]
impl ShellManager {
    /// Create a new shell manager
    pub fn new() -> Self {
        Self {
            shells: Mutex::new(HashMap::new()),
            exec_outputs: Mutex::new(HashMap::new()),
            exec_exits: Mutex::new(HashMap::new()),
        }
    }

    /// Spawn a new shell process for a channel with PTY support
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_shell(
        &self,
        channel_id: u32,
        shell: &str,
        channel: ChannelId,
        handle: Handle,
        env_vars: Vec<(String, String)>,
        term: Option<String>,
        pty_config: Option<PtyConfig>,
    ) -> anyhow::Result<()> {
        let term_type = term.unwrap_or_else(|| "xterm-256color".to_string());

        // If PTY is requested, use portable-pty
        if let Some(pty_cfg) = pty_config {
            self.spawn_shell_with_pty(
                channel_id, shell, channel, handle, env_vars, term_type, pty_cfg,
            )
            .await
        } else {
            // Fallback to non-PTY mode (pipe-based)
            self.spawn_shell_no_pty(channel_id, shell, channel, handle, env_vars, term_type)
                .await
        }
    }

    /// Spawn a shell with a real PTY
    #[allow(clippy::too_many_arguments)]
    async fn spawn_shell_with_pty(
        &self,
        channel_id: u32,
        shell: &str,
        channel: ChannelId,
        handle: Handle,
        env_vars: Vec<(String, String)>,
        term_type: String,
        pty_cfg: PtyConfig,
    ) -> anyhow::Result<()> {
        // Create PTY system
        let pty_system = native_pty_system();

        // Create the PTY pair with specified size
        let pair = pty_system.openpty(PtySize {
            rows: pty_cfg.rows,
            cols: pty_cfg.cols,
            pixel_width: pty_cfg.pixel_width,
            pixel_height: pty_cfg.pixel_height,
        })?;

        // Build the command
        let mut cmd = CommandBuilder::new(shell);
        cmd.arg("-i");

        // Set environment variables
        for (key, value) in env_vars {
            cmd.env(&key, &value);
        }
        cmd.env("TERM", &term_type);
        cmd.env("PS1", "\\u@sockrats:\\w\\$ ");

        // Spawn the child in the PTY
        let child = pair.slave.spawn_command(cmd)?;

        // Get the master PTY for reading/writing
        let master = pair.master;

        // Create channel for sending data to PTY stdin
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<Vec<u8>>(256);

        // Get a writer for the master PTY
        let mut pty_writer = master.take_writer()?;

        // Spawn task to write to PTY
        tokio::task::spawn_blocking(move || {
            while let Some(data) = stdin_rx.blocking_recv() {
                if pty_writer.write_all(&data).is_err() {
                    break;
                }
                let _ = pty_writer.flush();
            }
        });

        // Spawn task to read from PTY and send to channel
        let handle_pty = handle.clone();
        let channel_for_pty = channel;
        let mut pty_reader = master.try_clone_reader()?;

        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            let mut buf = [0u8; 4096];
            loop {
                match pty_reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let data = CryptoVec::from(&buf[..n]);
                        if rt.block_on(handle_pty.data(channel_for_pty, data)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        // On Linux, EIO means the PTY slave has closed
                        if e.raw_os_error() == Some(5) {
                            // EIO - normal PTY close
                            break;
                        }
                        // Other errors
                        tracing::debug!("PTY read error: {:?}", e);
                        break;
                    }
                }
            }
        });

        // Spawn task to wait for child exit and send exit status
        let handle_exit = handle.clone();
        let channel_for_exit = channel;
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            let mut child = child;

            // Wait for child process to exit
            let exit_status = match child.wait() {
                Ok(status) => {
                    if status.success() {
                        0u32
                    } else {
                        status.exit_code()
                    }
                }
                Err(_) => 1,
            };

            // Send exit status, EOF, and close channel
            let _ = rt.block_on(handle_exit.exit_status_request(channel_for_exit, exit_status));
            let _ = rt.block_on(handle_exit.eof(channel_for_exit));
            let _ = rt.block_on(handle_exit.close(channel_for_exit));
        });

        // Store the shell process
        let shell_process = ShellProcess { stdin_tx };

        let mut shells = self.shells.lock().await;
        shells.insert(channel_id, shell_process);

        Ok(())
    }

    /// Spawn a shell without PTY (pipe-based, for exec without pty_request)
    async fn spawn_shell_no_pty(
        &self,
        channel_id: u32,
        shell: &str,
        channel: ChannelId,
        handle: Handle,
        env_vars: Vec<(String, String)>,
        term_type: String,
    ) -> anyhow::Result<()> {
        let mut cmd = Command::new(shell);
        cmd.arg("-i");

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in env_vars {
            cmd.env(&key, &value);
        }
        cmd.env("TERM", &term_type);
        cmd.env("PS1", "\\u@sockrats:\\w\\$ ");

        let mut child = cmd.spawn()?;

        let stdin = child.stdin.take().expect("Failed to get stdin");
        let stdout = child.stdout.take().expect("Failed to get stdout");
        let stderr = child.stderr.take().expect("Failed to get stderr");

        let (stdin_tx, mut stdin_rx) = mpsc::channel::<Vec<u8>>(256);

        // Write to stdin
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(data) = stdin_rx.recv().await {
                if stdin.write_all(&data).await.is_err() {
                    break;
                }
                let _ = stdin.flush().await;
            }
        });

        // Read stdout
        let handle_stdout = handle.clone();
        let channel_for_stdout = channel;
        tokio::spawn(async move {
            let mut stdout = stdout;
            let mut buf = [0u8; 4096];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if handle_stdout
                            .data(channel_for_stdout, CryptoVec::from(&buf[..n]))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Read stderr
        let handle_stderr = handle.clone();
        let channel_for_stderr = channel;
        tokio::spawn(async move {
            let mut stderr = stderr;
            let mut buf = [0u8; 4096];
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if handle_stderr
                            .extended_data(channel_for_stderr, 1, CryptoVec::from(&buf[..n]))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Wait for child exit
        let handle_exit = handle.clone();
        let channel_for_exit = channel;
        tokio::spawn(async move {
            let mut child = child;
            let exit_status = match child.wait().await {
                Ok(status) => status.code().unwrap_or(1) as u32,
                Err(_) => 1,
            };
            let _ = handle_exit
                .exit_status_request(channel_for_exit, exit_status)
                .await;
            let _ = handle_exit.eof(channel_for_exit).await;
            let _ = handle_exit.close(channel_for_exit).await;
        });

        let shell_process = ShellProcess { stdin_tx };

        let mut shells = self.shells.lock().await;
        shells.insert(channel_id, shell_process);

        Ok(())
    }

    /// Write data to a shell's stdin
    pub async fn write_to_shell(&self, channel_id: u32, data: &[u8]) -> anyhow::Result<bool> {
        let shells = self.shells.lock().await;
        if let Some(shell) = shells.get(&channel_id) {
            shell.write(data).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Remove a shell process
    pub async fn remove_shell(&self, channel_id: u32) {
        let mut shells = self.shells.lock().await;
        shells.remove(&channel_id);
    }

    /// Check if a channel has an active shell
    pub async fn has_shell(&self, channel_id: u32) -> bool {
        let shells = self.shells.lock().await;
        shells.contains_key(&channel_id)
    }

    /// Spawn a subsystem process (e.g., sftp-server) with direct handle forwarding
    ///
    /// Unlike `spawn_exec()`, this forwards stdout and stderr separately to the
    /// SSH channel via handle, preserving the binary protocol on stdout.
    /// Stderr goes to extended_data (type 1) so it doesn't corrupt the protocol.
    /// Waits for stdout/stderr to drain before sending exit_status/eof/close.
    pub async fn spawn_subsystem(
        &self,
        channel_id: u32,
        command: &str,
        channel: ChannelId,
        handle: Handle,
    ) -> anyhow::Result<()> {
        let mut cmd = Command::new(command);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;

        let stdin = child.stdin.take().expect("Failed to get stdin");
        let stdout = child.stdout.take().expect("Failed to get stdout");
        let stderr = child.stderr.take().expect("Failed to get stderr");

        let (stdin_tx, mut stdin_rx) = mpsc::channel::<Vec<u8>>(256);

        // Write to stdin
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(data) = stdin_rx.recv().await {
                if stdin.write_all(&data).await.is_err() {
                    break;
                }
                let _ = stdin.flush().await;
            }
        });

        // Read stdout → handle.data() (binary protocol)
        let handle_stdout = handle.clone();
        let stdout_task = tokio::spawn(async move {
            let mut stdout = stdout;
            let mut buf = [0u8; 4096];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if handle_stdout
                            .data(channel, CryptoVec::from(&buf[..n]))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Read stderr → handle.extended_data() (error messages, separate from protocol)
        let handle_stderr = handle.clone();
        let stderr_task = tokio::spawn(async move {
            let mut stderr = stderr;
            let mut buf = [0u8; 4096];
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if handle_stderr
                            .extended_data(channel, 1, CryptoVec::from(&buf[..n]))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Wait for child exit, then drain readers, then send exit/eof/close
        let handle_exit = handle.clone();
        tokio::spawn(async move {
            let exit_status = match child.wait().await {
                Ok(status) => status.code().unwrap_or(1) as u32,
                Err(_) => 1,
            };
            // Wait for stdout/stderr readers to finish draining
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            // Now safe to signal completion
            let _ = handle_exit.exit_status_request(channel, exit_status).await;
            let _ = handle_exit.eof(channel).await;
            let _ = handle_exit.close(channel).await;
        });

        // Register process in shell manager (for write_to_shell from data() handler)
        let shell_process = ShellProcess { stdin_tx };
        self.shells.lock().await.insert(channel_id, shell_process);

        Ok(())
    }

    /// Spawn a command with streaming I/O (for SCP and other interactive exec)
    ///
    /// Unlike the one-shot `exec_request` that uses `Command::output()`, this
    /// spawns the command with piped stdin/stdout/stderr so that:
    /// - The SSH `data()` handler can feed stdin via `write_to_shell()`
    /// - Output is captured in a channel retrievable via `take_exec_output()`
    /// - Exit status is sent via a oneshot retrievable via `take_exec_exit()`
    pub async fn spawn_exec(
        &self,
        channel_id: u32,
        command: &str,
        default_shell: &str,
        env_vars: Vec<(String, String)>,
    ) -> anyhow::Result<()> {
        let mut cmd = Command::new(default_shell);
        cmd.arg("-c").arg(command);

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in &env_vars {
            cmd.env(key, value);
        }
        cmd.env("TERM", "xterm-256color");

        let mut child = cmd.spawn()?;

        let stdin = child.stdin.take().expect("Failed to get stdin");
        let stdout = child.stdout.take().expect("Failed to get stdout");
        let stderr = child.stderr.take().expect("Failed to get stderr");

        // Channel for writing to process stdin (from SSH data() handler)
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<Vec<u8>>(256);

        // Channel for capturing process output (stdout + stderr merged)
        let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>(256);

        // Oneshot for exit status
        let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<u32>();

        // Write to stdin
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(data) = stdin_rx.recv().await {
                if stdin.write_all(&data).await.is_err() {
                    break;
                }
                let _ = stdin.flush().await;
            }
        });

        // Read stdout → output channel
        let output_tx_stdout = output_tx.clone();
        tokio::spawn(async move {
            let mut stdout = stdout;
            let mut buf = [0u8; 4096];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if output_tx_stdout.send(buf[..n].to_vec()).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Read stderr → output channel (merged with stdout)
        let output_tx_stderr = output_tx;
        tokio::spawn(async move {
            let mut stderr = stderr;
            let mut buf = [0u8; 4096];
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if output_tx_stderr.send(buf[..n].to_vec()).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Wait for child exit and send exit status
        tokio::spawn(async move {
            let exit_status = match child.wait().await {
                Ok(status) => status.code().unwrap_or(1) as u32,
                Err(_) => 1,
            };
            let _ = exit_tx.send(exit_status);
        });

        // Register process in shell manager (for write_to_shell)
        let shell_process = ShellProcess { stdin_tx };
        self.shells.lock().await.insert(channel_id, shell_process);

        // Store output and exit receivers
        self.exec_outputs.lock().await.insert(channel_id, output_rx);
        self.exec_exits.lock().await.insert(channel_id, exit_rx);

        Ok(())
    }

    /// Take the output receiver for a streaming exec command
    ///
    /// Returns `None` if no exec output is registered for this channel.
    /// The receiver is removed from the manager (can only be taken once).
    pub async fn take_exec_output(&self, channel_id: u32) -> Option<mpsc::Receiver<Vec<u8>>> {
        self.exec_outputs.lock().await.remove(&channel_id)
    }

    /// Take the exit status receiver for a streaming exec command
    ///
    /// Returns `None` if no exec exit is registered for this channel.
    /// The receiver is removed from the manager (can only be taken once).
    pub async fn take_exec_exit(
        &self,
        channel_id: u32,
    ) -> Option<tokio::sync::oneshot::Receiver<u32>> {
        self.exec_exits.lock().await.remove(&channel_id)
    }
}

#[cfg(feature = "ssh")]
impl Default for ShellManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared shell manager type
#[cfg(feature = "ssh")]
pub type SharedShellManager = Arc<ShellManager>;

/// Create a new shared shell manager
#[cfg(feature = "ssh")]
pub fn new_shell_manager() -> SharedShellManager {
    Arc::new(ShellManager::new())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_module_compiles() {
        // This test ensures the module compiles
        assert!(true);
    }

    #[cfg(feature = "ssh")]
    #[test]
    fn test_shell_manager_creation() {
        use super::*;
        let _manager = ShellManager::new();
    }

    #[cfg(feature = "ssh")]
    #[test]
    fn test_pty_config_default() {
        use super::*;
        let cfg = PtyConfig::default();
        assert_eq!(cfg.cols, 80);
        assert_eq!(cfg.rows, 24);
    }

    #[cfg(feature = "ssh")]
    #[tokio::test]
    async fn test_spawn_exec_registers_in_manager() {
        use super::*;

        let manager = ShellManager::new();
        assert!(!manager.has_shell(42).await);

        // spawn_exec should register the command in the shell manager
        // so that data() handler can feed stdin to the process
        let result = manager
            .spawn_exec(42, "echo hello", "/bin/sh", vec![])
            .await;
        assert!(result.is_ok());
        assert!(manager.has_shell(42).await);

        // Cleanup
        manager.remove_shell(42).await;
    }

    #[cfg(feature = "ssh")]
    #[tokio::test]
    async fn test_spawn_exec_captures_output() {
        use super::*;

        let manager = ShellManager::new();

        let result = manager
            .spawn_exec(1, "echo SCP_TEST_OUTPUT", "/bin/sh", vec![])
            .await;
        assert!(result.is_ok());

        // The command should have been spawned and output captured
        // via the ExecOutput receiver
        let output = manager.take_exec_output(1).await;
        assert!(output.is_some());

        let mut rx = output.unwrap();
        // Wait for the command to produce output
        let data = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert!(
            String::from_utf8_lossy(&data).contains("SCP_TEST_OUTPUT"),
            "Expected output to contain SCP_TEST_OUTPUT, got: {:?}",
            String::from_utf8_lossy(&data)
        );

        manager.remove_shell(1).await;
    }

    #[cfg(feature = "ssh")]
    #[tokio::test]
    async fn test_spawn_exec_accepts_stdin() {
        use super::*;

        let manager = ShellManager::new();

        // `cat` will echo whatever we write to stdin
        let result = manager.spawn_exec(2, "cat", "/bin/sh", vec![]).await;
        assert!(result.is_ok());

        // Write data to the process via shell manager (like data() handler does)
        let write_ok = manager.write_to_shell(2, b"hello from scp\n").await;
        assert!(write_ok.is_ok());

        // Give process time to echo
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let output = manager.take_exec_output(2).await;
        assert!(output.is_some());

        let mut rx = output.unwrap();
        let data = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert!(
            String::from_utf8_lossy(&data).contains("hello from scp"),
            "Expected echo, got: {:?}",
            String::from_utf8_lossy(&data)
        );

        manager.remove_shell(2).await;
    }

    #[cfg(feature = "ssh")]
    #[tokio::test]
    async fn test_spawn_exec_with_env_vars() {
        use super::*;

        let manager = ShellManager::new();
        let env = vec![("MY_VAR".to_string(), "test_value".to_string())];

        let result = manager.spawn_exec(3, "echo $MY_VAR", "/bin/sh", env).await;
        assert!(result.is_ok());

        let output = manager.take_exec_output(3).await;
        assert!(output.is_some());

        let mut rx = output.unwrap();
        let data = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert!(
            String::from_utf8_lossy(&data).contains("test_value"),
            "Expected env var output, got: {:?}",
            String::from_utf8_lossy(&data)
        );

        manager.remove_shell(3).await;
    }

    #[cfg(feature = "ssh")]
    #[tokio::test]
    async fn test_spawn_exec_exit_status() {
        use super::*;

        let manager = ShellManager::new();

        // Run a command that exits with status 0
        let result = manager.spawn_exec(4, "true", "/bin/sh", vec![]).await;
        assert!(result.is_ok());

        let exit_rx = manager.take_exec_exit(4).await;
        assert!(exit_rx.is_some());

        let exit_code = tokio::time::timeout(std::time::Duration::from_secs(2), exit_rx.unwrap())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert_eq!(exit_code, 0);

        // Run a command that exits with status 1
        let result = manager.spawn_exec(5, "false", "/bin/sh", vec![]).await;
        assert!(result.is_ok());

        let exit_rx = manager.take_exec_exit(5).await;
        assert!(exit_rx.is_some());

        let exit_code = tokio::time::timeout(std::time::Duration::from_secs(2), exit_rx.unwrap())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert_ne!(exit_code, 0);
    }
}
