use std::sync::Arc;

use uuid::Uuid;

use crate::storage::Storage;

/// Log an auditable action to the database.
pub async fn audit_action(
    storage: &Arc<Storage>,
    user_id: Uuid,
    username: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
) {
    let _ = storage
        .create_audit_log(
            Some(user_id),
            username,
            action,
            Some(resource_type),
            Some(resource_id),
            None,
            None,
        )
        .await;
}
