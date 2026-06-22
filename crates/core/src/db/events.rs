//! Idempotent upsert of canonical [`Event`] rows into the partitioned `events` table.

use crate::events::{Category, Event, EventStatus, Severity, VerificationStatus};
use crate::Result;
use sqlx::PgPool;

/// Insert or update an event keyed on `(tenant_id, source, source_event_id, occurred_at)`.
pub async fn upsert_event(pool: &PgPool, event: &Event) -> Result<()> {
    let embedding = event
        .embedding
        .as_ref()
        .map(|values| format_pgvector(values.as_slice()));

    sqlx::query(
        r#"
        INSERT INTO events (
            id, tenant_id, source, source_event_id, category, severity, impact_score,
            magnitude, title, summary, body, original_text, translated_text, language,
            location, country, region, place_name,
            occurred_at, detected_at, ingested_at,
            status, verification_status, confidence,
            tags, url, raw, embedding
        ) VALUES (
            $1, $2, $3, $4,
            $5::event_category, $6::event_severity, $7,
            $8, $9, $10, $11, $12, $13, $14,
            ST_SetSRID(ST_MakePoint($15, $16), 4326)::geography,
            $17, $18, $19,
            $20, $21, $22,
            $23::event_status, $24::verification_status, $25,
            $26, $27, $28::jsonb, $29::vector
        )
        ON CONFLICT (tenant_id, source, source_event_id, occurred_at) DO UPDATE SET
            category = EXCLUDED.category,
            severity = EXCLUDED.severity,
            impact_score = EXCLUDED.impact_score,
            magnitude = EXCLUDED.magnitude,
            title = EXCLUDED.title,
            summary = EXCLUDED.summary,
            body = EXCLUDED.body,
            original_text = EXCLUDED.original_text,
            translated_text = EXCLUDED.translated_text,
            language = EXCLUDED.language,
            location = EXCLUDED.location,
            country = EXCLUDED.country,
            region = EXCLUDED.region,
            place_name = EXCLUDED.place_name,
            detected_at = EXCLUDED.detected_at,
            ingested_at = EXCLUDED.ingested_at,
            status = EXCLUDED.status,
            verification_status = EXCLUDED.verification_status,
            confidence = EXCLUDED.confidence,
            tags = EXCLUDED.tags,
            url = EXCLUDED.url,
            raw = EXCLUDED.raw,
            embedding = EXCLUDED.embedding
        "#,
    )
    .bind(event.id)
    .bind(event.tenant_id)
    .bind(&event.source)
    .bind(&event.source_event_id)
    .bind(pg_category(event.category))
    .bind(pg_severity(event.severity))
    .bind(i16::from(event.impact_score))
    .bind(event.magnitude)
    .bind(&event.title)
    .bind(&event.summary)
    .bind(&event.body)
    .bind(&event.original_text)
    .bind(&event.translated_text)
    .bind(&event.language)
    .bind(event.location.lon)
    .bind(event.location.lat)
    .bind(&event.country)
    .bind(&event.region)
    .bind(&event.place_name)
    .bind(event.occurred_at)
    .bind(event.detected_at)
    .bind(event.ingested_at)
    .bind(pg_status(event.status))
    .bind(pg_verification(event.verification_status))
    .bind(event.confidence)
    .bind(&event.tags)
    .bind(&event.url)
    .bind(&event.raw)
    .bind(embedding)
    .execute(pool)
    .await?;
    Ok(())
}

fn pg_category(value: Category) -> &'static str {
    match value {
        Category::Earthquake => "earthquake",
        Category::Incident => "incident",
        Category::Alert => "alert",
        Category::Weather => "weather",
        Category::News => "news",
        Category::Conflict => "conflict",
        Category::Wildfire => "wildfire",
        Category::Other => "other",
    }
}

fn pg_severity(value: Severity) -> &'static str {
    match value {
        Severity::Info => "info",
        Severity::Low => "low",
        Severity::Moderate => "moderate",
        Severity::High => "high",
        Severity::Critical => "critical",
    }
}

fn pg_status(value: EventStatus) -> &'static str {
    match value {
        EventStatus::Active => "active",
        EventStatus::Resolved => "resolved",
        EventStatus::Archived => "archived",
    }
}

fn pg_verification(value: VerificationStatus) -> &'static str {
    match value {
        VerificationStatus::Verified => "verified",
        VerificationStatus::Unverified => "unverified",
        VerificationStatus::Rumor => "rumor",
        VerificationStatus::Disputed => "disputed",
    }
}

fn format_pgvector(values: &[f32]) -> String {
    let body = values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!("[{body}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pg_enum_strings_match_migration() {
        assert_eq!(pg_category(Category::Earthquake), "earthquake");
        assert_eq!(pg_severity(Severity::Critical), "critical");
        assert_eq!(pg_status(EventStatus::Active), "active");
        assert_eq!(
            pg_verification(VerificationStatus::Unverified),
            "unverified"
        );
    }
}
