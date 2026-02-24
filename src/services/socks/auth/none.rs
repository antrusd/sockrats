//! No authentication handler
//!
//! Handles the case when no authentication is required.

use anyhow::Result;
use tokio::io::{AsyncRead, AsyncWrite};

/// No authentication handler
///
/// This is a marker type for the no-authentication method.
/// Since no authentication is required, this doesn't do anything.
pub struct NoAuth;

impl NoAuth {
    /// Perform "authentication" (which does nothing)
    pub async fn authenticate<S>(_stream: &mut S) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        // No authentication required - nothing to do
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn test_no_auth() {
        let mut stream = Cursor::new(Vec::new());
        let result = NoAuth::authenticate(&mut stream).await;
        assert!(result.is_ok());
    }
}
