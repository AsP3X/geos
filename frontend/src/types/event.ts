/* AUTO-GENERATED from crates/core/schema/event.schema.json by scripts/gen-types.sh. DO NOT EDIT. */

/**
 * The canonical event record.
 *
 * This is the serialization contract shared with the frontend and CLI. The
 * `raw` field always retains the original source payload; source-specific data
 * lives there rather than forking this type.
 */
export interface Event {
  /**
   * Optional affected area as a GeoJSON Polygon (SRID 4326).
   */
  affected_area?: {
    [k: string]: unknown;
  };
  /**
   * Full body text in the user's language (translated if needed).
   */
  body?: string | null;
  /**
   * High-level category.
   */
  category: "earthquake" | "incident" | "alert" | "weather" | "news" | "conflict" | "wildfire" | "other";
  /**
   * Confidence in [0, 1] (distinct from source reliability).
   */
  confidence: number;
  /**
   * ISO 3166-1 alpha-2 country code, when resolved.
   */
  country?: string | null;
  /**
   * When the source first detected it, if distinct from `occurred_at`.
   */
  detected_at?: string | null;
  /**
   * Semantic embedding (length [`EMBEDDING_DIM`]); absent until enriched.
   */
  embedding?: number[] | null;
  /**
   * Stable unique identifier.
   */
  id: string;
  /**
   * Normalized 0–100 impact score (deterministic, versioned scoring).
   */
  impact_score: number;
  /**
   * When Geos ingested it.
   */
  ingested_at: string;
  /**
   * BCP-47 language tag of the original source text (e.g. `"en"`).
   */
  language?: string | null;
  location: GeoPoint;
  /**
   * Optional source magnitude (e.g. earthquake moment magnitude).
   */
  magnitude?: number | null;
  /**
   * When the event actually occurred (partition key, monthly partitions).
   */
  occurred_at: string;
  /**
   * Original-language source text, retained alongside any translation.
   */
  original_text?: string | null;
  /**
   * Human-readable place name, when resolved.
   */
  place_name?: string | null;
  /**
   * Original source payload (jsonb), always retained.
   */
  raw: {
    [k: string]: unknown;
  };
  /**
   * Administrative region / state, when resolved.
   */
  region?: string | null;
  /**
   * Coarse severity tier.
   */
  severity: "info" | "low" | "moderate" | "high" | "critical";
  /**
   * Source system identifier (e.g. `"usgs"`, `"manual"`).
   */
  source: string;
  /**
   * Source-native event id; `(tenant_id, source, source_event_id)` is the
   * idempotency key for upserts (`connector-contract.mdc`).
   */
  source_event_id: string;
  /**
   * Lifecycle status.
   */
  status: "active" | "resolved" | "archived";
  /**
   * One- or two-sentence summary (may be AI-generated).
   */
  summary?: string | null;
  /**
   * Free-form tags for filtering.
   */
  tags: string[];
  /**
   * Owning tenant (every row is tenant-scoped; `tenant-isolation.mdc`).
   */
  tenant_id: string;
  /**
   * Short human title.
   */
  title?: string | null;
  /**
   * Translated text, when the source was in another language.
   */
  translated_text?: string | null;
  /**
   * Canonical source URL, when available.
   */
  url?: string | null;
  /**
   * Trust/verification state.
   */
  verification_status: "verified" | "unverified" | "rumor" | "disputed";
  [k: string]: unknown;
}
/**
 * Primary point location (SRID 4326).
 */
export interface GeoPoint {
  /**
   * Latitude in decimal degrees, range [-90, 90].
   */
  lat: number;
  /**
   * Longitude in decimal degrees, range [-180, 180].
   */
  lon: number;
  [k: string]: unknown;
}
