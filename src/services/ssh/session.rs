//! SSH session management
//!
//! This module manages SSH session state and channel handling.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Channel type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    /// Interactive session
    Session,
    /// Direct TCP/IP forwarding
    DirectTcpip,
    /// Forwarded TCP/IP
    ForwardedTcpip,
    /// X11 forwarding
    X11,
}

/// Channel state
#[derive(Debug, Clone)]
pub struct ChannelState {
    /// Channel type
    pub channel_type: ChannelType,
    /// Whether PTY is allocated
    pub pty_allocated: bool,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// PTY terminal type
    pub term: Option<String>,
    /// PTY dimensions (cols, rows, pxwidth, pxheight)
    pub pty_size: Option<(u32, u32, u32, u32)>,
}

impl ChannelState {
    /// Create a new session channel state
    pub fn new_session() -> Self {
        Self {
            channel_type: ChannelType::Session,
            pty_allocated: false,
            env: HashMap::new(),
            term: None,
            pty_size: None,
        }
    }

    /// Create a new direct TCP/IP channel state
    pub fn new_direct_tcpip() -> Self {
        Self {
            channel_type: ChannelType::DirectTcpip,
            pty_allocated: false,
            env: HashMap::new(),
            term: None,
            pty_size: None,
        }
    }

    /// Set PTY parameters
    pub fn set_pty(&mut self, term: String, cols: u32, rows: u32, pxwidth: u32, pxheight: u32) {
        self.pty_allocated = true;
        self.term = Some(term);
        self.pty_size = Some((cols, rows, pxwidth, pxheight));
    }

    /// Update window size
    pub fn update_window_size(&mut self, cols: u32, rows: u32, pxwidth: u32, pxheight: u32) {
        self.pty_size = Some((cols, rows, pxwidth, pxheight));
    }

    /// Set environment variable
    pub fn set_env(&mut self, name: String, value: String) {
        self.env.insert(name, value);
    }

    /// Get environment variables as a Vec of (name, value) tuples
    pub fn env_vars(&self) -> Vec<(String, String)> {
        self.env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

/// SSH session state
#[derive(Debug)]
pub struct SessionState {
    /// Whether the user is authenticated
    pub authenticated: bool,
    /// Username (if authenticated)
    pub username: Option<String>,
    /// Active channels
    pub channels: HashMap<u32, ChannelState>,
    /// Failed authentication attempts
    pub auth_attempts: u32,
    /// Maximum authentication attempts
    pub max_auth_attempts: u32,
}

impl SessionState {
    /// Create a new session state
    pub fn new(max_auth_attempts: u32) -> Self {
        Self {
            authenticated: false,
            username: None,
            channels: HashMap::new(),
            auth_attempts: 0,
            max_auth_attempts,
        }
    }

    /// Mark authentication as successful
    pub fn authenticate(&mut self, username: String) {
        self.authenticated = true;
        self.username = Some(username);
    }

    /// Record a failed authentication attempt
    pub fn record_auth_failure(&mut self) {
        self.auth_attempts += 1;
    }

    /// Check if max auth attempts exceeded
    pub fn auth_attempts_exceeded(&self) -> bool {
        self.auth_attempts >= self.max_auth_attempts
    }

    /// Add a new channel
    pub fn add_channel(&mut self, channel_id: u32, state: ChannelState) {
        self.channels.insert(channel_id, state);
    }

    /// Get a channel
    pub fn get_channel(&self, channel_id: u32) -> Option<&ChannelState> {
        self.channels.get(&channel_id)
    }

    /// Get a mutable channel
    pub fn get_channel_mut(&mut self, channel_id: u32) -> Option<&mut ChannelState> {
        self.channels.get_mut(&channel_id)
    }

    /// Remove a channel
    pub fn remove_channel(&mut self, channel_id: u32) -> Option<ChannelState> {
        self.channels.remove(&channel_id)
    }

    /// Get number of active channels
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }
}

/// Thread-safe session state wrapper
pub type SharedSessionState = Arc<Mutex<SessionState>>;

/// Create a new shared session state
pub fn new_shared_session(max_auth_attempts: u32) -> SharedSessionState {
    Arc::new(Mutex::new(SessionState::new(max_auth_attempts)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_state_new_session() {
        let state = ChannelState::new_session();
        assert_eq!(state.channel_type, ChannelType::Session);
        assert!(!state.pty_allocated);
        assert!(state.env.is_empty());
        assert!(state.term.is_none());
        assert!(state.pty_size.is_none());
    }

    #[test]
    fn test_channel_state_new_direct_tcpip() {
        let state = ChannelState::new_direct_tcpip();
        assert_eq!(state.channel_type, ChannelType::DirectTcpip);
    }

    #[test]
    fn test_channel_state_set_pty() {
        let mut state = ChannelState::new_session();
        state.set_pty("xterm-256color".to_string(), 80, 24, 640, 480);
        assert!(state.pty_allocated);
        assert_eq!(state.term, Some("xterm-256color".to_string()));
        assert_eq!(state.pty_size, Some((80, 24, 640, 480)));
    }

    #[test]
    fn test_channel_state_update_window_size() {
        let mut state = ChannelState::new_session();
        state.set_pty("xterm".to_string(), 80, 24, 640, 480);
        state.update_window_size(120, 40, 960, 800);
        assert_eq!(state.pty_size, Some((120, 40, 960, 800)));
    }

    #[test]
    fn test_channel_state_set_env() {
        let mut state = ChannelState::new_session();
        state.set_env("HOME".to_string(), "/home/user".to_string());
        state.set_env("PATH".to_string(), "/usr/bin".to_string());
        assert_eq!(state.env.get("HOME"), Some(&"/home/user".to_string()));
        assert_eq!(state.env.get("PATH"), Some(&"/usr/bin".to_string()));
    }

    #[test]
    fn test_session_state_new() {
        let state = SessionState::new(6);
        assert!(!state.authenticated);
        assert!(state.username.is_none());
        assert!(state.channels.is_empty());
        assert_eq!(state.auth_attempts, 0);
        assert_eq!(state.max_auth_attempts, 6);
    }

    #[test]
    fn test_session_state_authenticate() {
        let mut state = SessionState::new(6);
        state.authenticate("admin".to_string());
        assert!(state.authenticated);
        assert_eq!(state.username, Some("admin".to_string()));
    }

    #[test]
    fn test_session_state_auth_attempts() {
        let mut state = SessionState::new(3);
        assert!(!state.auth_attempts_exceeded());

        state.record_auth_failure();
        assert!(!state.auth_attempts_exceeded());

        state.record_auth_failure();
        assert!(!state.auth_attempts_exceeded());

        state.record_auth_failure();
        assert!(state.auth_attempts_exceeded());
    }

    #[test]
    fn test_session_state_channels() {
        let mut state = SessionState::new(6);

        state.add_channel(0, ChannelState::new_session());
        state.add_channel(1, ChannelState::new_direct_tcpip());
        assert_eq!(state.channel_count(), 2);

        let ch0 = state.get_channel(0).unwrap();
        assert_eq!(ch0.channel_type, ChannelType::Session);

        let ch1 = state.get_channel_mut(1).unwrap();
        ch1.set_env("TEST".to_string(), "value".to_string());

        let removed = state.remove_channel(0);
        assert!(removed.is_some());
        assert_eq!(state.channel_count(), 1);
    }

    #[test]
    fn test_new_shared_session() {
        let state = new_shared_session(6);
        let guard = state.try_lock().unwrap();
        assert!(!guard.authenticated);
    }

    #[test]
    fn test_channel_type_eq() {
        assert_eq!(ChannelType::Session, ChannelType::Session);
        assert_ne!(ChannelType::Session, ChannelType::DirectTcpip);
    }

    #[test]
    fn test_channel_type_clone_copy() {
        let ct = ChannelType::Session;
        let ct2 = ct;
        let ct3 = ct.clone();
        assert_eq!(ct, ct2);
        assert_eq!(ct, ct3);
    }

    #[test]
    fn test_channel_state_clone() {
        let mut state = ChannelState::new_session();
        state.set_pty("xterm".to_string(), 80, 24, 640, 480);
        state.set_env("HOME".to_string(), "/home".to_string());

        let cloned = state.clone();
        assert_eq!(cloned.term, state.term);
        assert_eq!(cloned.pty_size, state.pty_size);
        assert_eq!(cloned.env, state.env);
    }
}
