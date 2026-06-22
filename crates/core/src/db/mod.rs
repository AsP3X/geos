//! SQLx database access: connection pool, migrations, and tenant-scoped persistence.

mod auth_tokens;
mod connectors;
mod events;
mod notify;
mod pool;
mod tasks;
mod tenants;
mod users;

pub use auth_tokens::{
    find_valid_refresh_token, insert_refresh_token, revoke_refresh_token, RefreshTokenRow,
};
pub use connectors::touch_live_run;
pub use events::{get_event, list_events, upsert_event, EventBBox, EventListFilter};
pub use notify::{
    notify_event_upsert, EventNotifyAction, EventNotifyPayload, EVENT_NOTIFY_CHANNEL,
};
pub use pool::{connect_pool, run_migrations, PgPool};
pub use tasks::{
    claim_task, complete_task, enqueue_task, fail_task, EnqueueTask, WorkerTask, WorkerTaskStatus,
};
pub use tenants::{
    create_tenant_with_roles, find_membership_for_user, find_tenant_id_by_slug, insert_membership,
    list_memberships_for_user, permissions_for_role, register_tenant_and_owner, MembershipAuthRow,
};
pub use users::{find_user_by_email, insert_user, UserAuthRow};

use crate::AppError;

impl From<sqlx::Error> for AppError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value.to_string())
    }
}

impl From<sqlx::migrate::MigrateError> for AppError {
    fn from(value: sqlx::migrate::MigrateError) -> Self {
        Self::Database(value.to_string())
    }
}
