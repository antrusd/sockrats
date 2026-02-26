//! SSH Handler implementation
//!
//! This module implements the russh `Handler` trait for SSH server functionality.

#[cfg(feature = "ssh")]
use super::auth::verify_password;
use super::auth::PublicKeyAuth;
use super::config::SshConfig;
#[cfg(feature = "ssh")]
use super::process::{new_shell_manager, PtyConfig, SharedShellManager};
#[cfg(feature = "ssh")]
use super::session::{new_shared_session, ChannelState, SharedSessionState};
use std::sync::Arc;

#[cfg(feature = "ssh")]
use russh::keys::PublicKey;
#[cfg(feature = "ssh")]
use russh::server::{Auth, Handler, Msg, Session};
#[cfg(feature = "ssh")]
use russh::{Channel, ChannelId, CryptoVec};

/// SSH server handler
#[cfg(feature = "ssh")]
pub struct SshHandler {
    /// SSH configuration
    config: Arc<SshConfig>,
    /// Public key authenticator
    pubkey_auth: Option<PublicKeyAuth>,
    /// Session state
    session_state: SharedSessionState,
    /// Shell process manager
    shell_manager: SharedShellManager,
}

#[cfg(feature = "ssh")]
impl SshHandler {
    /// Create a new SSH handler
    pub fn new(config: Arc<SshConfig>, pubkey_auth: Option<PublicKeyAuth>) -> Self {
        let max_auth_attempts = config.max_auth_tries;
        Self {
            config,
            pubkey_auth,
            session_state: new_shared_session(max_auth_attempts),
            shell_manager: new_shell_manager(),
        }
    }
}

#[cfg(feature = "ssh")]
impl Handler for SshHandler {
    type Error = anyhow::Error;

    /// Handle password authentication
    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        tracing::debug!(username = %user, "Password authentication attempt");

        // Check if max attempts exceeded
        {
            let state = self.session_state.lock().await;
            if state.auth_attempts_exceeded() {
                tracing::warn!("Max authentication attempts exceeded");
                return Ok(Auth::reject());
            }
        }

        if verify_password(&self.config, user, password) {
            let mut state = self.session_state.lock().await;
            state.authenticate(user.to_string());
            tracing::info!(username = %user, "Password authentication successful");
            Ok(Auth::Accept)
        } else {
            let mut state = self.session_state.lock().await;
            state.record_auth_failure();
            tracing::warn!(username = %user, "Password authentication failed");
            Ok(Auth::reject())
        }
    }

    /// Handle public key authentication (check if key is acceptable)
    async fn auth_publickey_offered(
        &mut self,
        user: &str,
        public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        tracing::debug!(username = %user, "Public key offered");

        if !self.config.has_publickey_auth() {
            return Ok(Auth::reject());
        }

        // Check if the key is in authorized_keys
        if let Some(ref auth) = self.pubkey_auth {
            if auth.is_authorized(public_key) {
                // Key is acceptable, but signature not yet verified
                return Ok(Auth::Accept);
            }
        }

        Ok(Auth::reject())
    }

    /// Handle public key authentication (after signature verification)
    async fn auth_publickey(
        &mut self,
        user: &str,
        public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        tracing::debug!(username = %user, "Public key authentication");

        // Check if max attempts exceeded
        {
            let state = self.session_state.lock().await;
            if state.auth_attempts_exceeded() {
                tracing::warn!("Max authentication attempts exceeded");
                return Ok(Auth::reject());
            }
        }

        if !self.config.has_publickey_auth() {
            let mut state = self.session_state.lock().await;
            state.record_auth_failure();
            return Ok(Auth::reject());
        }

        // Verify the key is authorized
        if let Some(ref auth) = self.pubkey_auth {
            if auth.is_authorized(public_key) {
                let mut state = self.session_state.lock().await;
                state.authenticate(user.to_string());
                tracing::info!(username = %user, "Public key authentication successful");
                return Ok(Auth::Accept);
            }
        }

        let mut state = self.session_state.lock().await;
        state.record_auth_failure();
        tracing::warn!(username = %user, "Public key authentication failed");
        Ok(Auth::reject())
    }

    /// Handle channel open session request
    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let state = self.session_state.lock().await;
        if !state.authenticated {
            tracing::warn!("Channel open rejected: not authenticated");
            return Ok(false);
        }
        drop(state);

        let channel_id: u32 = channel.id().into();
        tracing::debug!(channel_id, "Session channel opened");

        let mut state = self.session_state.lock().await;
        state.add_channel(channel_id, ChannelState::new_session());

        Ok(true)
    }

    /// Handle PTY request
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if !self.config.pty {
            tracing::warn!("PTY request rejected: PTY disabled");
            session.channel_failure(channel)?;
            return Ok(());
        }

        let channel_id: u32 = channel.into();
        tracing::debug!(
            channel_id,
            term,
            cols = col_width,
            rows = row_height,
            "PTY request"
        );

        let mut state = self.session_state.lock().await;
        if let Some(ch) = state.get_channel_mut(channel_id) {
            ch.set_pty(
                term.to_string(),
                col_width,
                row_height,
                pix_width,
                pix_height,
            );
            session.channel_success(channel)?;
        } else {
            session.channel_failure(channel)?;
        }

        Ok(())
    }

    /// Handle shell request
    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if !self.config.shell {
            tracing::warn!("Shell request rejected: shell disabled");
            session.channel_failure(channel)?;
            return Ok(());
        }

        let channel_id: u32 = channel.into();
        tracing::info!(channel_id, "Shell request");

        // Get handle for async operations
        let handle = session.handle();

        // Get channel state for env vars, terminal info, and PTY config
        let (env_vars, term, pty_config) = {
            let state = self.session_state.lock().await;
            let ch = state.get_channel(channel_id);
            let env_vars = ch.map(|c| c.env_vars()).unwrap_or_default();
            let term = ch.and_then(|c| c.term.clone());
            let pty_config = ch.and_then(|c| {
                if c.pty_allocated {
                    c.pty_size.map(|(cols, rows, pxwidth, pxheight)| PtyConfig {
                        cols: cols as u16,
                        rows: rows as u16,
                        pixel_width: pxwidth as u16,
                        pixel_height: pxheight as u16,
                    })
                } else {
                    None
                }
            });
            (env_vars, term, pty_config)
        };

        // Spawn the shell process
        let shell = &self.config.default_shell;
        match self
            .shell_manager
            .spawn_shell(
                channel_id, shell, channel, handle, env_vars, term, pty_config,
            )
            .await
        {
            Ok(()) => {
                tracing::info!(channel_id, ?shell, "Shell spawned successfully");
                session.channel_success(channel)?;
            }
            Err(e) => {
                tracing::error!(channel_id, error = %e, "Failed to spawn shell");
                session.channel_failure(channel)?;
                let error_msg = format!("Failed to spawn shell: {}\r\n", e);
                session.data(channel, CryptoVec::from(error_msg.as_bytes()))?;
            }
        }

        Ok(())
    }

    /// Handle exec request (streaming I/O for SCP and interactive commands)
    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if !self.config.exec {
            tracing::warn!("Exec request rejected: exec disabled");
            session.channel_failure(channel)?;
            return Ok(());
        }

        let command = String::from_utf8_lossy(data).to_string();
        let channel_id: u32 = channel.into();
        tracing::info!(channel_id, command = %command, "Exec request");

        // Get environment variables for this channel
        let env_vars = {
            let state = self.session_state.lock().await;
            state
                .get_channel(channel_id)
                .map(|ch| ch.env_vars().to_vec())
                .unwrap_or_default()
        };

        // Spawn the command with streaming I/O (supports SCP bidirectional protocol)
        let default_shell = &self.config.default_shell;
        match self
            .shell_manager
            .spawn_exec(channel_id, &command, default_shell, env_vars)
            .await
        {
            Ok(()) => {
                session.channel_success(channel)?;

                // Take the output and exit receivers
                let output_rx = self.shell_manager.take_exec_output(channel_id).await;
                let exit_rx = self.shell_manager.take_exec_exit(channel_id).await;

                let handle = session.handle();

                // Single task: drain output, then wait for exit, then close
                tokio::spawn(async move {
                    // Forward all process output to SSH channel
                    if let Some(mut output_rx) = output_rx {
                        while let Some(data) = output_rx.recv().await {
                            if handle
                                .data(channel, CryptoVec::from(data.as_slice()))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }

                    // Output is fully drained — now send exit status and close
                    let exit_code = match exit_rx {
                        Some(rx) => rx.await.unwrap_or(1),
                        None => 1,
                    };
                    let _ = handle.exit_status_request(channel, exit_code).await;
                    let _ = handle.eof(channel).await;
                    let _ = handle.close(channel).await;
                });
            }
            Err(e) => {
                tracing::error!(channel_id, error = %e, "Failed to spawn exec");
                session.channel_success(channel)?;
                let error_msg = format!("Failed to execute command: {}\r\n", e);
                session.data(channel, CryptoVec::from(error_msg.as_bytes()))?;
                session.exit_status_request(channel, 1)?;
                session.close(channel)?;
            }
        }

        Ok(())
    }

    /// Handle environment variable request
    async fn env_request(
        &mut self,
        channel: ChannelId,
        variable_name: &str,
        variable_value: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let channel_id: u32 = channel.into();
        tracing::debug!(
            channel_id,
            name = variable_name,
            value = variable_value,
            "Env request"
        );

        let mut state = self.session_state.lock().await;
        if let Some(ch) = state.get_channel_mut(channel_id) {
            ch.set_env(variable_name.to_string(), variable_value.to_string());
            session.channel_success(channel)?;
        } else {
            session.channel_failure(channel)?;
        }

        Ok(())
    }

    /// Handle window change request
    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let channel_id: u32 = channel.into();
        tracing::debug!(
            channel_id,
            cols = col_width,
            rows = row_height,
            "Window change"
        );

        let mut state = self.session_state.lock().await;
        if let Some(ch) = state.get_channel_mut(channel_id) {
            ch.update_window_size(col_width, row_height, pix_width, pix_height);
        }

        Ok(())
    }

    /// Handle channel EOF from client (client finished sending data)
    ///
    /// This drops the stdin writer, which signals EOF to the subprocess.
    /// The subprocess can then finish processing and exit, triggering
    /// exit_status/eof/close back to the client.
    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let channel_id: u32 = channel.into();
        tracing::debug!(channel_id, "Channel EOF received");

        // Drop stdin writer to signal EOF to the subprocess
        self.shell_manager.remove_shell(channel_id).await;

        Ok(())
    }

    /// Handle channel close
    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let channel_id: u32 = channel.into();
        tracing::debug!(channel_id, "Channel closed");

        // Clean up shell process if any (in case EOF wasn't received)
        self.shell_manager.remove_shell(channel_id).await;

        let mut state = self.session_state.lock().await;
        state.remove_channel(channel_id);

        Ok(())
    }

    /// Handle data from client
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let channel_id: u32 = channel.into();
        tracing::trace!(channel_id, len = data.len(), "Data received");

        // Forward data to shell if one exists for this channel
        if self.shell_manager.has_shell(channel_id).await {
            if let Err(e) = self.shell_manager.write_to_shell(channel_id, data).await {
                tracing::warn!(channel_id, error = %e, "Failed to write to shell");
            }
        }
        // Note: We don't echo data back - the shell's stdout reader will send output

        Ok(())
    }

    /// Handle subsystem request (e.g., SFTP)
    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let channel_id: u32 = channel.into();
        tracing::info!(channel_id, subsystem = name, "Subsystem request");

        match name {
            "sftp" if self.config.sftp => {
                let sftp_server = &self.config.sftp_server;
                tracing::info!(channel_id, sftp_server, "Spawning SFTP subsystem");

                let handle = session.handle();

                // Spawn sftp-server with direct handle forwarding
                // (binary protocol on stdout, stderr separate via extended_data)
                match self
                    .shell_manager
                    .spawn_subsystem(channel_id, sftp_server, channel, handle)
                    .await
                {
                    Ok(()) => {
                        session.channel_success(channel)?;
                    }
                    Err(e) => {
                        tracing::error!(
                            channel_id,
                            error = %e,
                            sftp_server,
                            "Failed to spawn SFTP subsystem"
                        );
                        session.channel_failure(channel)?;
                    }
                }
            }
            _ => {
                tracing::warn!(subsystem = name, "Unknown or disabled subsystem");
                session.channel_failure(channel)?;
            }
        }

        Ok(())
    }
}

/// Placeholder handler for when SSH feature is disabled
#[cfg(not(feature = "ssh"))]
pub struct SshHandler;

#[cfg(not(feature = "ssh"))]
impl SshHandler {
    /// Create a new placeholder SSH handler (SSH feature disabled)
    pub fn new(_config: Arc<SshConfig>, _pubkey_auth: Option<PublicKeyAuth>) -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_compiles_without_ssh_feature() {
        // This test ensures the module compiles without the ssh feature
        let config = Arc::new(SshConfig::default());
        #[cfg(feature = "ssh")]
        {
            let _handler = SshHandler::new(config, None);
        }
        #[cfg(not(feature = "ssh"))]
        {
            let _handler = SshHandler::new(config, None);
        }
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_ssh_handler_creation() {
        let config = Arc::new(SshConfig::default());
        let handler = SshHandler::new(config.clone(), None);
        assert!(Arc::ptr_eq(&handler.config, &config));
    }

    #[test]
    #[cfg(feature = "ssh")]
    fn test_ssh_handler_with_pubkey_auth() {
        use super::super::auth::AuthorizedKeys;

        let config = Arc::new(SshConfig {
            enabled: true,
            auth_methods: vec!["publickey".to_string()],
            ..Default::default()
        });
        let pubkey_auth = Some(PublicKeyAuth::new(AuthorizedKeys::new()));
        let handler = SshHandler::new(config, pubkey_auth);
        assert!(handler.pubkey_auth.is_some());
    }
}
