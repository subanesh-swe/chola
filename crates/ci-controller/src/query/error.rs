//! ChQL error types. Carries a byte-offset position for editor underline UX.

use crate::api::error::ApiError;

#[derive(Debug, Clone, PartialEq)]
pub struct ChqlError {
    pub message: String,
    pub position: Option<usize>,
    pub hint: Option<String>,
}

impl ChqlError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            position: None,
            hint: None,
        }
    }

    pub fn at(message: impl Into<String>, position: usize) -> Self {
        Self {
            message: message.into(),
            position: Some(position),
            hint: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl std::fmt::Display for ChqlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.position {
            Some(pos) => write!(f, "ChQL error at position {pos}: {}", self.message),
            None => write!(f, "ChQL error: {}", self.message),
        }?;
        if let Some(hint) = &self.hint {
            write!(f, " (hint: {hint})")?;
        }
        Ok(())
    }
}

impl std::error::Error for ChqlError {}

impl From<ChqlError> for ApiError {
    fn from(err: ChqlError) -> Self {
        let mut msg = err.message.clone();
        if let Some(pos) = err.position {
            msg = format!("{msg} (position {pos})");
        }
        if let Some(hint) = err.hint {
            msg = format!("{msg}: {hint}");
        }
        ApiError::BadRequest(msg)
    }
}
