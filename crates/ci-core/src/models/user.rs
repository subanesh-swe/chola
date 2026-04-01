use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    SuperAdmin,
    Admin,
    Operator,
    Viewer,
}

impl UserRole {
    pub fn from_db_str(s: &str) -> Self {
        match s {
            "super_admin" => Self::SuperAdmin,
            "admin" => Self::Admin,
            "operator" => Self::Operator,
            _ => Self::Viewer,
        }
    }

    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::SuperAdmin => "super_admin",
            Self::Admin => "admin",
            Self::Operator => "operator",
            Self::Viewer => "viewer",
        }
    }

    pub fn can_manage_users(&self) -> bool {
        matches!(self, Self::SuperAdmin)
    }
    pub fn can_manage_repos(&self) -> bool {
        matches!(self, Self::SuperAdmin | Self::Admin)
    }
    pub fn can_cancel_jobs(&self) -> bool {
        matches!(self, Self::SuperAdmin | Self::Admin)
    }
    pub fn can_trigger_builds(&self) -> bool {
        !matches!(self, Self::Viewer)
    }
    pub fn can_manage_workers(&self) -> bool {
        matches!(self, Self::SuperAdmin | Self::Admin)
    }
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_db_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub display_name: Option<String>,
    pub role: UserRole,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
