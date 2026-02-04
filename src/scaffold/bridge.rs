//! Generic process bridge for external scaffold integration.
//!
//! This module provides a reusable abstraction for communicating with external
//! processes (like SWE-Agent, OpenHands, etc.) via stdin/stdout.

use std::fmt;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

/// Errors that can occur during process bridge operations.
#[derive(Debug, Clone)]
pub enum BridgeError {
    /// Failed to spawn the subprocess.
    SpawnFailed(String),
    /// Process exited unexpectedly with the given exit code.
    ProcessExited(i32),
    /// Operation timed out.
    Timeout,
    /// I/O error during communication.
    IoError(String),
    /// Failed to parse process output.
    ParseError(String),
    /// Process is not running.
    NotRunning,
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BridgeError::SpawnFailed(msg) => write!(f, "Failed to spawn process: {}", msg),
            BridgeError::ProcessExited(code) => {
                write!(f, "Process exited unexpectedly with code {}", code)
            }
            BridgeError::Timeout => write!(f, "Operation timed out"),
            BridgeError::IoError(msg) => write!(f, "I/O error: {}", msg),
            BridgeError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            BridgeError::NotRunning => write!(f, "Process is not running"),
        }
    }
}

impl std::error::Error for BridgeError {}

/// A bridge for communicating with external processes via stdin/stdout.
///
/// The `ProcessBridge` manages the lifecycle of a subprocess, providing
/// async methods for sending messages and receiving responses.
///
/// # Example
///
/// ```ignore
/// use dataforge::scaffold::bridge::ProcessBridge;
/// use std::time::Duration;
///
/// let bridge = ProcessBridge::spawn(
///     "python3",
///     &["-u", "script.py"],
///     &[("PYTHONUNBUFFERED", "1")],
///     Duration::from_secs(30),
/// ).await?;
///
/// let response = bridge.send_receive("hello\n").await?;
/// ```
pub struct ProcessBridge {
    process: Child,
    stdin: ChildStdin,
    stdout_reader: BufReader<ChildStdout>,
    timeout_duration: Duration,
}

impl ProcessBridge {
    /// Spawn a new subprocess and create a bridge for communication.
    ///
    /// # Arguments
    ///
    /// * `cmd` - The command/executable to run
    /// * `args` - Arguments to pass to the command
    /// * `env` - Environment variables to set for the subprocess
    /// * `timeout_duration` - Default timeout for operations
    ///
    /// # Returns
    ///
    /// A `ProcessBridge` connected to the subprocess, or an error if spawn fails.
    pub async fn spawn(
        cmd: &str,
        args: &[&str],
        env: &[(&str, &str)],
        timeout_duration: Duration,
    ) -> Result<Self, BridgeError> {
        let mut command = Command::new(cmd);
        command
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // Set environment variables
        for (key, value) in env {
            command.env(key, value);
        }

        let mut process = command
            .spawn()
            .map_err(|e| BridgeError::SpawnFailed(e.to_string()))?;

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| BridgeError::SpawnFailed("Failed to capture stdin".to_string()))?;

        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| BridgeError::SpawnFailed("Failed to capture stdout".to_string()))?;

        let stdout_reader = BufReader::new(stdout);

        Ok(Self {
            process,
            stdin,
            stdout_reader,
            timeout_duration,
        })
    }

    /// Send a message to the subprocess via stdin.
    ///
    /// # Arguments
    ///
    /// * `message` - The message to send (should include newline if needed)
    ///
    /// # Returns
    ///
    /// `Ok(())` if the message was sent successfully, or an error.
    pub async fn send(&mut self, message: &str) -> Result<(), BridgeError> {
        // Check if process is still running
        if !self.is_running() {
            return Err(BridgeError::NotRunning);
        }

        let send_future = async {
            self.stdin
                .write_all(message.as_bytes())
                .await
                .map_err(|e| BridgeError::IoError(e.to_string()))?;
            self.stdin
                .flush()
                .await
                .map_err(|e| BridgeError::IoError(e.to_string()))?;
            Ok(())
        };

        timeout(self.timeout_duration, send_future)
            .await
            .map_err(|_| BridgeError::Timeout)?
    }

    /// Receive a line of output from the subprocess.
    ///
    /// Reads a single line from stdout, waiting up to the configured timeout.
    ///
    /// # Returns
    ///
    /// The line read (without trailing newline), or an error.
    pub async fn receive(&mut self) -> Result<String, BridgeError> {
        let receive_future = async {
            let mut line = String::new();
            let bytes_read = self
                .stdout_reader
                .read_line(&mut line)
                .await
                .map_err(|e| BridgeError::IoError(e.to_string()))?;

            if bytes_read == 0 {
                // EOF - process likely exited
                return Err(BridgeError::ProcessExited(
                    self.process
                        .try_wait()
                        .ok()
                        .flatten()
                        .map(|s| s.code().unwrap_or(-1))
                        .unwrap_or(-1),
                ));
            }

            // Remove trailing newline
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }

            Ok(line)
        };

        timeout(self.timeout_duration, receive_future)
            .await
            .map_err(|_| BridgeError::Timeout)?
    }

    /// Receive multiple lines until a delimiter is encountered.
    ///
    /// Keeps reading lines until a line matching the delimiter is found.
    ///
    /// # Arguments
    ///
    /// * `delimiter` - The line that signals end of output
    ///
    /// # Returns
    ///
    /// All lines read (excluding the delimiter), or an error.
    pub async fn receive_until(&mut self, delimiter: &str) -> Result<Vec<String>, BridgeError> {
        let mut lines = Vec::new();

        loop {
            let line = self.receive().await?;
            if line == delimiter {
                break;
            }
            lines.push(line);
        }

        Ok(lines)
    }

    /// Send a message and receive a single-line response.
    ///
    /// This is a convenience method that combines `send` and `receive`.
    ///
    /// # Arguments
    ///
    /// * `message` - The message to send
    ///
    /// # Returns
    ///
    /// The response line, or an error.
    pub async fn send_receive(&mut self, message: &str) -> Result<String, BridgeError> {
        self.send(message).await?;
        self.receive().await
    }

    /// Send a message and receive a multi-line response.
    ///
    /// # Arguments
    ///
    /// * `message` - The message to send
    /// * `delimiter` - The line that signals end of response
    ///
    /// # Returns
    ///
    /// All response lines (excluding delimiter), or an error.
    pub async fn send_receive_until(
        &mut self,
        message: &str,
        delimiter: &str,
    ) -> Result<Vec<String>, BridgeError> {
        self.send(message).await?;
        self.receive_until(delimiter).await
    }

    /// Close the bridge and terminate the subprocess.
    ///
    /// This sends SIGTERM to the process and waits for it to exit.
    /// If the process doesn't exit within the timeout, it will be killed.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the process was terminated successfully, or an error.
    pub async fn close(mut self) -> Result<(), BridgeError> {
        // Shutdown stdin to signal we're done (without moving)
        self.stdin
            .shutdown()
            .await
            .map_err(|e| BridgeError::IoError(e.to_string()))?;

        // Try to wait for graceful exit
        let wait_result = timeout(Duration::from_secs(5), self.process.wait()).await;

        match wait_result {
            Ok(Ok(_status)) => Ok(()),
            Ok(Err(e)) => Err(BridgeError::IoError(e.to_string())),
            Err(_) => {
                // Timeout - force kill
                self.process
                    .kill()
                    .await
                    .map_err(|e| BridgeError::IoError(format!("Failed to kill process: {}", e)))?;
                Ok(())
            }
        }
    }

    /// Check if the subprocess is still running.
    ///
    /// # Returns
    ///
    /// `true` if the process is running, `false` if it has exited.
    pub fn is_running(&mut self) -> bool {
        match self.process.try_wait() {
            Ok(None) => true,     // Still running
            Ok(Some(_)) => false, // Exited
            Err(_) => false,      // Error checking - assume not running
        }
    }

    /// Get the process ID of the subprocess.
    ///
    /// # Returns
    ///
    /// The PID, or `None` if the process has exited.
    pub fn pid(&self) -> Option<u32> {
        self.process.id()
    }

    /// Update the default timeout for operations.
    ///
    /// # Arguments
    ///
    /// * `timeout_duration` - The new timeout duration
    pub fn set_timeout(&mut self, timeout_duration: Duration) {
        self.timeout_duration = timeout_duration;
    }

    /// Get the current timeout duration.
    pub fn timeout(&self) -> Duration {
        self.timeout_duration
    }

    /// Drain any pending output from stdout without blocking.
    ///
    /// This is useful for clearing the buffer before sending a new command.
    ///
    /// # Returns
    ///
    /// Lines that were pending, or an error.
    pub async fn drain(&mut self) -> Result<Vec<String>, BridgeError> {
        let mut lines = Vec::new();

        // Use a very short timeout for draining
        let drain_timeout = Duration::from_millis(100);

        loop {
            let receive_future = async {
                let mut line = String::new();
                let bytes_read = self
                    .stdout_reader
                    .read_line(&mut line)
                    .await
                    .map_err(|e| BridgeError::IoError(e.to_string()))?;

                if bytes_read == 0 {
                    return Err(BridgeError::ProcessExited(-1));
                }

                if line.ends_with('\n') {
                    line.pop();
                }
                Ok(line)
            };

            match timeout(drain_timeout, receive_future).await {
                Ok(Ok(line)) => lines.push(line),
                Ok(Err(BridgeError::ProcessExited(_))) => break,
                Ok(Err(_)) => break,
                Err(_) => break, // Timeout - no more pending output
            }
        }

        Ok(lines)
    }
}

impl Drop for ProcessBridge {
    fn drop(&mut self) {
        // Attempt to kill the process when the bridge is dropped
        // We can't await in drop, so we just start the kill
        let _ = self.process.start_kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_error_display() {
        assert_eq!(
            BridgeError::SpawnFailed("test".to_string()).to_string(),
            "Failed to spawn process: test"
        );
        assert_eq!(
            BridgeError::ProcessExited(1).to_string(),
            "Process exited unexpectedly with code 1"
        );
        assert_eq!(BridgeError::Timeout.to_string(), "Operation timed out");
        assert_eq!(
            BridgeError::IoError("test".to_string()).to_string(),
            "I/O error: test"
        );
        assert_eq!(
            BridgeError::ParseError("test".to_string()).to_string(),
            "Parse error: test"
        );
        assert_eq!(
            BridgeError::NotRunning.to_string(),
            "Process is not running"
        );
    }

    #[tokio::test]
    async fn test_spawn_nonexistent_command() {
        let result = ProcessBridge::spawn(
            "/nonexistent/command/that/does/not/exist",
            &[],
            &[],
            Duration::from_secs(5),
        )
        .await;

        assert!(result.is_err());
        match result {
            Err(BridgeError::SpawnFailed(_)) => {}
            _ => panic!("Expected SpawnFailed error"),
        }
    }

    #[tokio::test]
    async fn test_spawn_echo_command() {
        // Test with a simple echo command that exits immediately
        let mut bridge = ProcessBridge::spawn("echo", &["hello"], &[], Duration::from_secs(5))
            .await
            .expect("Failed to spawn echo");

        let line = bridge.receive().await.expect("Failed to receive");
        assert_eq!(line, "hello");

        // Process should exit after echo
        let _ = bridge.close().await;
    }

    #[tokio::test]
    async fn test_bridge_timeout() {
        // Use cat which waits for input - it won't output anything without input
        let result = ProcessBridge::spawn(
            "cat",
            &[],
            &[],
            Duration::from_millis(100), // Very short timeout
        )
        .await;

        if let Ok(mut bridge) = result {
            // Try to receive without sending anything - should timeout
            let receive_result = bridge.receive().await;
            assert!(matches!(receive_result, Err(BridgeError::Timeout)));
            let _ = bridge.close().await;
        }
    }

    #[tokio::test]
    async fn test_bridge_send_receive() {
        // Use cat to echo back what we send
        let mut bridge = ProcessBridge::spawn("cat", &[], &[], Duration::from_secs(5))
            .await
            .expect("Failed to spawn cat");

        bridge.send("test message\n").await.expect("Failed to send");
        let response = bridge.receive().await.expect("Failed to receive");
        assert_eq!(response, "test message");

        let _ = bridge.close().await;
    }

    #[tokio::test]
    async fn test_bridge_is_running() {
        let mut bridge = ProcessBridge::spawn("cat", &[], &[], Duration::from_secs(5))
            .await
            .expect("Failed to spawn cat");

        assert!(bridge.is_running());
        assert!(bridge.pid().is_some());

        let _ = bridge.close().await;
    }

    #[test]
    fn test_bridge_error_is_error_trait() {
        fn assert_error<T: std::error::Error>() {}
        assert_error::<BridgeError>();
    }
}
