//! The canonical [`Event`] domain model — the single source of truth for every
//! ingested record in Geos (`canonical-event-schema.mdc`).
//!
//! Connectors normalize raw source data into this shape, the API serves it, the
//! frontend consumes it (via generated TS types), and the CLI prints it. Any
//! change here must be mirrored in the exported JSON Schema, the generated
//! TypeScript types, and a new sqlx migration in the same change set.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Dimension of the event `embedding` vector.
///
/// Fixed for the lifetime of the schema: it must match both the `vector(N)`
/// column in the migration and the embedding model's output size
/// (`geospatial-postgis.mdc`). Changing it is a breaking schema change.
pub const EMBEDDING_DIM: usize = 1024;

/// High-level classification of an event. Closed, versioned enum: adding a
/// value requires updating the DB enum, migrations, and UI legend/filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Seismic event (e.g. USGS earthquake).
    Earthquake,
    /// Generic incident not covered by a more specific category.
    Incident,
    /// Official alert or warning.
    Alert,
    /// Weather-related event.
    Weather,
    /// News-derived event.
    News,
    /// Armed conflict or violence.
    Conflict,
    /// Wildfire / bushfire.
    Wildfire,
    /// Anything not matching the above.
    Other,
}

/// Coarse severity tier, complementing the numeric `impact_score`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Informational only.
    Info,
    /// Low severity.
    Low,
    /// Moderate severity.
    Moderate,
    /// High severity.
    High,
    /// Critical severity.
    Critical,
}

/// Lifecycle status of an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    /// Currently active / ongoing.
    Active,
    /// Resolved / concluded.
    Resolved,
    /// Archived (retained but inactive).
    Archived,
}

/// Trust/verification state, set by ingestion or the annotation workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    /// Confirmed by a trusted source or analyst.
    Verified,
    /// Not yet verified (default).
    Unverified,
    /// Unconfirmed rumor.
    Rumor,
    /// Conflicting information; disputed.
    Disputed,
}

/// A geographic point in WGS84 (SRID 4326).
///
/// Stored in PostGIS as `geography(Point,4326)`. Field order is (lon, lat) to
/// match PostGIS constructors (`geospatial-postgis.mdc`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeoPoint {
    /// Longitude in decimal degrees, range [-180, 180].
    pub lon: f64,
    /// Latitude in decimal degrees, range [-90, 90].
    pub lat: f64,
}

/// The canonical event record.
///
/// This is the serialization contract shared with the frontend and CLI. The
/// `raw` field always retains the original source payload; source-specific data
/// lives there rather than forking this type.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    /// Stable unique identifier.
    pub id: Uuid,
    /// Owning tenant (every row is tenant-scoped; `tenant-isolation.mdc`).
    pub tenant_id: Uuid,

    /// Source system identifier (e.g. `"usgs"`, `"manual"`).
    pub source: String,
    /// Source-native event id; `(tenant_id, source, source_event_id)` is the
    /// idempotency key for upserts (`connector-contract.mdc`).
    pub source_event_id: String,

    /// High-level category.
    pub category: Category,
    /// Coarse severity tier.
    pub severity: Severity,
    /// Normalized 0–100 impact score (deterministic, versioned scoring).
    pub impact_score: u8,
    /// Optional source magnitude (e.g. earthquake moment magnitude).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magnitude: Option<f64>,

    /// Short human title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// One- or two-sentence summary (may be AI-generated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Full body text in the user's language (translated if needed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Original-language source text, retained alongside any translation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_text: Option<String>,
    /// Translated text, when the source was in another language.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translated_text: Option<String>,
    /// BCP-47 language tag of the original source text (e.g. `"en"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Primary point location (SRID 4326).
    pub location: GeoPoint,
    /// Optional affected area as a GeoJSON Polygon (SRID 4326).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_area: Option<serde_json::Value>,
    /// ISO 3166-1 alpha-2 country code, when resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// Administrative region / state, when resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// Human-readable place name, when resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place_name: Option<String>,

    /// When the event actually occurred (partition key, monthly partitions).
    pub occurred_at: DateTime<Utc>,
    /// When the source first detected it, if distinct from `occurred_at`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_at: Option<DateTime<Utc>>,
    /// When Geos ingested it.
    pub ingested_at: DateTime<Utc>,

    /// Lifecycle status.
    pub status: EventStatus,
    /// Trust/verification state.
    pub verification_status: VerificationStatus,
    /// Confidence in [0, 1] (distinct from source reliability).
    pub confidence: f32,

    /// Free-form tags for filtering.
    pub tags: Vec<String>,
    /// Canonical source URL, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Original source payload (jsonb), always retained.
    pub raw: serde_json::Value,
    /// Semantic embedding (length [`EMBEDDING_DIM`]); absent until enriched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}
