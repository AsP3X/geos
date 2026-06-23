//! Per-user saved filter presets and active-filter state.
//!
//! Both tables are tenant- and user-scoped (`tenant-isolation.mdc`). The stored
//! payload is the validated [`SavedFilterPayload`] (the canonical, versioned
//! filter shape) rather than opaque jsonb, so malformed presets are rejected at
//! the boundary. Mirrors the frontend `EventFilters` model (camelCase wire
//! format); time-range presets are resolved to `occurred_after/before` on the
//! client, so the backend only stores and validates them here.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::events::EventSort;
use crate::events::{Category, Severity};
use crate::{AppError, Result};

/// Time-range preset selected in the UI. `Custom` uses explicit `from`/`to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TimeRangePreset {
    /// Last hour.
    #[serde(rename = "1h")]
    OneHour,
    /// Last 24 hours.
    #[serde(rename = "24h")]
    TwentyFourHours,
    /// Last 7 days.
    #[serde(rename = "7d")]
    SevenDays,
    /// Last 30 days (default).
    #[serde(rename = "30d")]
    #[default]
    ThirtyDays,
    /// All time (no bound).
    #[serde(rename = "all")]
    All,
    /// Explicit custom range via `from`/`to`.
    #[serde(rename = "custom")]
    Custom,
}

/// Default impact ceiling (full range is 0–100).
const fn full_impact() -> u8 {
    100
}

/// Current stored-filter schema version.
const fn current_version() -> i32 {
    1
}

/// Validated filter payload persisted for presets and active state. Wire format
/// matches the frontend `EventFilters` (camelCase).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFilterPayload {
    /// Forward-compat schema version.
    #[serde(default = "current_version")]
    pub version: i32,
    /// Selected categories; empty = all.
    #[serde(default)]
    pub categories: Vec<Category>,
    /// Selected severities; empty = all.
    #[serde(default)]
    pub severities: Vec<Severity>,
    /// Selected source keys; empty = all.
    #[serde(default)]
    pub sources: Vec<String>,
    /// Lower impact bound (0–100).
    #[serde(default)]
    pub impact_min: u8,
    /// Upper impact bound (0–100).
    #[serde(default = "full_impact")]
    pub impact_max: u8,
    /// Lower magnitude bound; `None` = no lower bound.
    #[serde(default)]
    pub magnitude_min: Option<f64>,
    /// Upper magnitude bound; `None` = no upper bound.
    #[serde(default)]
    pub magnitude_max: Option<f64>,
    /// Time-range preset.
    #[serde(default)]
    pub time_range: TimeRangePreset,
    /// Custom-range start (ISO-8601) when `time_range = custom`.
    #[serde(default)]
    pub from: Option<DateTime<Utc>>,
    /// Custom-range end (ISO-8601) when `time_range = custom`.
    #[serde(default)]
    pub to: Option<DateTime<Utc>>,
    /// Result ordering.
    #[serde(default)]
    pub sort: EventSort,
}

impl SavedFilterPayload {
    /// Reject malformed payloads (out-of-range bounds, inverted ranges,
    /// custom range missing/inverted endpoints).
    pub fn validate(&self) -> Result<()> {
        if self.impact_min > 100 || self.impact_max > 100 {
            return Err(AppError::bad_request("impact bounds must be within 0–100"));
        }
        if self.impact_min > self.impact_max {
            return Err(AppError::bad_request(
                "impactMin must be less than or equal to impactMax",
            ));
        }
        if let (Some(min), Some(max)) = (self.magnitude_min, self.magnitude_max) {
            if min > max {
                return Err(AppError::bad_request(
                    "magnitudeMin must be less than or equal to magnitudeMax",
                ));
            }
        }
        if self.time_range == TimeRangePreset::Custom {
            match (self.from, self.to) {
                (Some(from), Some(to)) if from <= to => {}
                (Some(_), Some(_)) => {
                    return Err(AppError::bad_request("custom range: from must be <= to"));
                }
                _ => {
                    return Err(AppError::bad_request(
                        "custom time range requires from and to",
                    ));
                }
            }
        }
        Ok(())
    }
}

/// A stored preset row returned to API clients.
#[derive(Debug, Clone, Serialize)]
pub struct SavedFilter {
    /// Preset id.
    pub id: Uuid,
    /// User-visible name (unique per user within a tenant).
    pub name: String,
    /// The captured filter.
    pub filters: SavedFilterPayload,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last update time.
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct SavedFilterRow {
    id: Uuid,
    name: String,
    filters: Json<SavedFilterPayload>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<SavedFilterRow> for SavedFilter {
    fn from(row: SavedFilterRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            filters: row.filters.0,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// List the caller's presets, newest update first.
pub async fn list_saved_filters(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<Vec<SavedFilter>> {
    let rows = sqlx::query_as::<_, SavedFilterRow>(
        r#"
        SELECT id, name, filters, created_at, updated_at
        FROM saved_filters
        WHERE tenant_id = $1 AND user_id = $2
        ORDER BY updated_at DESC
        "#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(SavedFilter::from).collect())
}

/// Create a preset, overwriting any existing one with the same name for this
/// user (upsert on `(tenant_id, user_id, name)`).
pub async fn create_saved_filter(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    name: &str,
    payload: &SavedFilterPayload,
) -> Result<SavedFilter> {
    let row = sqlx::query_as::<_, SavedFilterRow>(
        r#"
        INSERT INTO saved_filters (tenant_id, user_id, name, filters)
        VALUES ($1, $2, $3, $4::jsonb)
        ON CONFLICT (tenant_id, user_id, name) DO UPDATE SET
            filters = EXCLUDED.filters,
            updated_at = now()
        RETURNING id, name, filters, created_at, updated_at
        "#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(name)
    .bind(Json(payload))
    .fetch_one(pool)
    .await?;
    Ok(row.into())
}

/// Update a preset by id (ownership-checked). Returns `None` if the preset does
/// not belong to the caller (treated as not-found to avoid existence leaks).
pub async fn update_saved_filter(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    id: Uuid,
    name: &str,
    payload: &SavedFilterPayload,
) -> Result<Option<SavedFilter>> {
    let row = sqlx::query_as::<_, SavedFilterRow>(
        r#"
        UPDATE saved_filters
        SET name = $4, filters = $5::jsonb, updated_at = now()
        WHERE id = $3 AND tenant_id = $1 AND user_id = $2
        RETURNING id, name, filters, created_at, updated_at
        "#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(id)
    .bind(name)
    .bind(Json(payload))
    .fetch_optional(pool)
    .await?;
    Ok(row.map(SavedFilter::from))
}

/// Delete a preset by id (ownership-checked). Returns `true` if a row was
/// removed, `false` if it did not belong to the caller.
pub async fn delete_saved_filter(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    id: Uuid,
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        DELETE FROM saved_filters
        WHERE id = $3 AND tenant_id = $1 AND user_id = $2
        "#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Read the caller's active filter state, if any has been persisted.
pub async fn get_user_filter_state(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<Option<SavedFilterPayload>> {
    let row = sqlx::query_scalar::<_, Json<SavedFilterPayload>>(
        r#"
        SELECT filters
        FROM user_filter_state
        WHERE tenant_id = $1 AND user_id = $2
        "#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|json| json.0))
}

/// Upsert the caller's active filter state (server is the source of truth).
pub async fn upsert_user_filter_state(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    payload: &SavedFilterPayload,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO user_filter_state (tenant_id, user_id, filters)
        VALUES ($1, $2, $3::jsonb)
        ON CONFLICT (tenant_id, user_id) DO UPDATE SET
            filters = EXCLUDED.filters,
            updated_at = now()
        "#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(Json(payload))
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn payload() -> SavedFilterPayload {
        SavedFilterPayload {
            version: 1,
            categories: vec![],
            severities: vec![],
            sources: vec![],
            impact_min: 0,
            impact_max: 100,
            magnitude_min: None,
            magnitude_max: None,
            time_range: TimeRangePreset::ThirtyDays,
            from: None,
            to: None,
            sort: EventSort::Recent,
        }
    }

    #[test]
    fn defaults_validate() {
        assert!(payload().validate().is_ok());
    }

    #[test]
    fn inverted_impact_range_is_rejected() {
        let mut p = payload();
        p.impact_min = 80;
        p.impact_max = 20;
        assert!(p.validate().is_err());
    }

    #[test]
    fn custom_range_requires_endpoints() {
        let mut p = payload();
        p.time_range = TimeRangePreset::Custom;
        assert!(p.validate().is_err());
    }

    #[test]
    fn wire_format_is_camel_case() {
        let json = serde_json::to_value(payload()).expect("serialize");
        assert!(json.get("impactMax").is_some());
        assert!(json.get("timeRange").is_some());
        assert_eq!(json.get("timeRange").and_then(|v| v.as_str()), Some("30d"));
    }

    #[test]
    fn unknown_keys_and_missing_fields_use_defaults() {
        // Forward-compat: extra keys ignored, missing keys take defaults.
        let parsed: SavedFilterPayload =
            serde_json::from_str(r#"{"categories":["earthquake"],"future":42}"#).expect("parse");
        assert_eq!(parsed.impact_max, 100);
        assert_eq!(parsed.time_range, TimeRangePreset::ThirtyDays);
        assert_eq!(parsed.sort, EventSort::Recent);
    }
}
