//! SQLx database access: connection pool, migrations, and tenant-scoped persistence.

mod auth_tokens;
mod connectors;
mod events;
mod notify;
mod pool;
mod saved_filters;
mod tasks;
mod tenants;
mod users;

pub use auth_tokens::{
    find_valid_refresh_token, insert_refresh_token, revoke_refresh_token, RefreshTokenRow,
};
pub use connectors::{
    add_backfill_events_ingested, advance_backfill_count, advance_backfill_cursor,
    ensure_event_partitions, get_connector_state, init_backfill_window, list_connector_states,
    mark_backfill_complete, reconcile_backfill_window, touch_live_run, ConnectorState,
};
pub use events::{
    count_events, get_event, list_event_map_points, list_event_sources, list_event_weather_areas,
    list_events, upsert_event, upsert_events_backfill, EventBBox, EventListFilter, EventListResult,
    EventMapPoint, EventMapResult, EventSort, EventWeatherArea, EventWeatherMapResult,
    WEATHER_MAP_MAX_LIMIT,
};
pub use notify::{
    notify_event_upsert, EventNotifyAction, EventNotifyPayload, EVENT_NOTIFY_CHANNEL,
};
pub use pool::{connect_pool, run_migrations, PgPool};
pub use saved_filters::{
    create_saved_filter, delete_saved_filter, get_user_filter_state, list_saved_filters,
    update_saved_filter, upsert_user_filter_state, SavedFilter, SavedFilterPayload,
};
pub use tasks::{
    claim_task, complete_task, enqueue_task, fail_task, recover_stale_tasks, EnqueueTask,
    WorkerTask, WorkerTaskStatus,
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
