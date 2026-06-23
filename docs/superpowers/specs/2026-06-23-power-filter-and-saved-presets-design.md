# Power filter, saved presets, and filter persistence — design

Date: 2026-06-23
Status: Proposed (awaiting user review)
Area: `frontend/` (filter UI), `crates/api` (events/search/saved-filters/sources/auth), `crates/core` (events query, Meili index, saved-filters + filter-state db), `migrations/`

## 1. Overview

The current event filter exposes only single-select category, single-select severity,
and a min-impact slider (`frontend/src/components/filters/FilterPanel.tsx`). The backend
events query already supports a time range and a bounding box that the UI never uses.

This work replaces the filter with a far more capable one (multi-select, ranges, time,
sort), adds **named saved presets** stored server-side (gated by the existing
`saved_filters.*` RBAC permissions), and makes the **active filter persist per user on
the server** (with a localStorage cache for instant first paint). Filtering happens
**server-side** so results stay correct regardless of page size.

## 2. Goals / non-goals

### Goals
- Multi-dimensional event filtering: time range, multi categories, multi severities,
  multi sources, impact range, magnitude range, sort.
- Server-side filtering + load-more pagination (correct results beyond a page cap).
- Named, per-user saved presets (CRUD, RBAC-gated, audit-logged).
- Server-authoritative active-filter persistence with a localStorage fast-path cache.
- Filters also constrain header search (where the Meilisearch index supports it).
- One-way sync: globe layers follow the active filter.

### Non-goals
- General settings/preferences area (theme, units, refresh rate). Out of scope.
- Tenant-shared presets (presets are per-user in v1; sharing is a future additive flag).
- Viewport ("search this map area") and status/verification filters (deferred).
- Boolean AND/OR filter logic (flat conjunctive filter only).
- Driving exports from filters (future; the schema leaves room).

## 3. Resolved decisions

| Topic | Decision |
|---|---|
| Dimensions | time range, multi categories, multi severities, multi sources, impact range (min+max), magnitude range (min+max), sort |
| Default time range | Last 7 days; custom range entered in local time, converted to UTC |
| Default categories | **All** categories (list/detail/search show everything; globe draws quakes only until other layers exist) |
| Magnitude semantics | When a magnitude range is active, events with **no** magnitude are excluded |
| Sort options | `recent` (occurred_at desc, default), `impact_desc`, `magnitude_desc`; nulls sort last |
| Result volume | **Load-more** pagination via existing `limit`/`offset` |
| Source options | New **dynamic endpoint** returning distinct sources present in the tenant's data |
| Search integration | Filters apply to search where the Meili index supports it; `source` + `occurred_at` added as filterable; search sort = recent + impact only (magnitude not indexed, so it does not constrain search) |
| Presets storage | New `saved_filters` table, **per-user private**, RBAC `saved_filters.read`/`saved_filters.manage`, audit-logged, overwrite-on-name-conflict |
| Preset contents | Captures the **entire** filter incl. time range and sort (fully reproducible view); JSON carries a `version` |
| Active-filter persistence | New `user_filter_state` table (per-user jsonb, versioned) is the **source of truth**; localStorage caches it for instant first paint and is reconciled with the server on load |
| Permission gating | Login/refresh **session carries the user's permissions** so the UI can show/hide preset Save/Delete controls |
| Globe layers | **One-way** sync: excluding `earthquake` hides quake dots/heat; excluding `weather` hides the weather layer; categories with no layer are unaffected. Replaces the current "default to earthquake" fetch hack |
| Live stream | Incoming events are filtered client-side against the active filter; auto-inserted only when sort = `recent`; for other sorts a "new events" refresh badge is shown |

## 4. Shared filter model

A single shape drives the query params, presets, persistence, and (where supported) search:

```ts
type TimeRangePreset = "1h" | "24h" | "7d" | "30d" | "all" | "custom";
type EventSort = "recent" | "impact_desc" | "magnitude_desc";

interface EventFilters {
  version: number;            // forward-compat; sanitized on load
  categories: Category[];     // empty = all
  severities: Severity[];     // empty = all
  sources: string[];          // empty = all
  impactMin: number;          // 0..100
  impactMax: number;          // 0..100
  magnitudeMin: number | null;
  magnitudeMax: number | null;
  timeRange: TimeRangePreset; // resolves to occurred_after/before
  from: string | null;        // ISO, used when timeRange = "custom"
  to: string | null;          // ISO, used when timeRange = "custom"
  sort: EventSort;
}
```

Defaults: all categories/severities/sources, impact 0–100, magnitude null/null,
`timeRange = "7d"`, `sort = "recent"`. Unknown/invalid values are sanitized to defaults
on load (versioned migration of stored JSON).

## 5. Backend — events query (`crates/core/src/db/events.rs`, `crates/api/src/routes/events.rs`)

- Extend `EventListFilter`: `categories: Vec<Category>`, `severities: Vec<Severity>`,
  `sources: Vec<String>`, `impact_min/impact_max: Option<u8>`,
  `min_magnitude/max_magnitude: Option<f64>`, `sort: EventSort`. Keep `occurred_after`/
  `occurred_before`, `bbox`, `limit`, `offset`.
- Build SQL with `category = ANY($n)`, `severity = ANY($n)`, `source = ANY($n)` (skip
  the predicate when the vec is empty). Impact/magnitude become `>=`/`<=` bounds; an
  active magnitude bound implies `magnitude IS NOT NULL`. `ORDER BY` switches on `sort`
  (`occurred_at DESC` default; `impact_score DESC NULLS LAST`; `magnitude DESC NULLS LAST`).
  Preserve `occurred_at` constraints for partition pruning.
- Extend `ListEventsQuery`: accept comma-separated `category`, `severity`, `source`,
  plus `impact_min`, `impact_max`, `min_magnitude`, `max_magnitude`, `sort`. Single values
  still parse (backward compatible). Validate ranges (min ≤ max, impact 0–100, known sort).
- **No `Event` schema change**, so the canonical four-artifact sync does not apply.

### Indexes (`migrations/0011_events_filter_sort_indexes.sql`)
Add btree indexes to keep sort fast across monthly partitions, e.g.
`(tenant_id, impact_score DESC)` and `(magnitude DESC)` (per-partition strategy consistent
with `geospatial-postgis.mdc`).

## 6. Backend — distinct sources endpoint

`GET /api/v1/events/sources` → `{ sources: string[] }`, tenant-scoped (caller's tenant +
system tenant), gated by `events.read`. Returns the distinct `source` values present in the
visible data so the source filter is always accurate (covers connector keys and `manual`).

## 7. Backend — search (`crates/api/src/routes/search.rs`, `crates/core/src/meili`)

- Extend `SearchQuery` + `SearchFilters` to accept multi `category`/`severity`, `sources`,
  `impact_min`/`impact_max`, and time range; build Meili filter expressions
  (`category IN [...]`, `source IN [...]`, `occurred_at >= …`, etc.).
- Add `source` to `EventDocument` and update the index settings:
  filterable `["tenant_id","category","severity","impact_score","occurred_at","source"]`;
  sortable stays `["occurred_at","impact_score"]`.
- Search sort offers `recent` + `impact_desc` only; magnitude is not indexed and is not
  applied to search.

## 8. Backend — saved filters + active filter state

### `migrations/0009_saved_filters.sql`
```
saved_filters(
  id uuid pk, tenant_id uuid not null, user_id uuid not null,
  name text not null, filters jsonb not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  unique (tenant_id, user_id, name)
)
index (tenant_id, user_id)
```

### `migrations/0010_user_filter_state.sql`
```
user_filter_state(
  tenant_id uuid not null, user_id uuid not null,
  filters jsonb not null,
  updated_at timestamptz not null default now(),
  primary key (tenant_id, user_id)
)
```

### Core db layer
Typed `SavedFilterPayload` (mirrors `EventFilters`, validated — not opaque jsonb). CRUD
helpers for `saved_filters` and an upsert/read for `user_filter_state`, all tenant + user
scoped.

### API routes
- `GET    /api/v1/saved-filters` — list caller's presets (`saved_filters.read`).
- `POST   /api/v1/saved-filters` — create `{name, filters}` (`saved_filters.manage`);
  name conflict → overwrite after client confirm (upsert on `(tenant_id,user_id,name)`).
- `PUT    /api/v1/saved-filters/{id}` — update (`saved_filters.manage`, ownership check).
- `DELETE /api/v1/saved-filters/{id}` — delete (`saved_filters.manage`, ownership check).
- `GET    /api/v1/filter-state` — read caller's active filter.
- `PUT    /api/v1/filter-state` — upsert caller's active filter.
- Cross-tenant/cross-user objects return **404** (no existence leak).
- Mutations write audit rows: `saved_filter.create|update|delete`. (Active-filter upserts
  are high-frequency UI state and are exempt from audit.)
- Error envelope follows `AppError` (`api-error-shape.mdc`).

## 9. Backend — session permissions (`crates/api` auth, `frontend` auth-storage)

Login/refresh responses include the authenticated user's permission keys (and role).
`AuthSession` + `sessionFromResponse` parse and store them. The UI gates preset Save/
Update/Delete on `saved_filters.manage`; apply is allowed with `saved_filters.read`.

## 10. Frontend

- **`filters.ts`**: new `EventFilters` model, defaults, sanitizer/versioned-migrator,
  `filtersAreActive`, and a `resolveTimeRange()` that maps preset → `occurred_after/before`.
- **`FilterPanel.tsx`** (rewrite): multi-select chips (category, severity, source), dual-range
  sliders (impact, magnitude), time-range segmented control + custom date inputs, sort
  dropdown, removable active-filter chips summary, reset. Source options come from the new
  sources endpoint.
- **Presets bar**: list/apply presets, Save-as (name), Update, Delete, set default; Save/
  Update/Delete hidden without `saved_filters.manage`. Backed by new `saved-filters-api.ts`.
- **`events-api.ts`**: serialize new params (comma-separated multis; resolved time range);
  expose `listEventSources()`.
- **`filter-state-api.ts`**: get/put active filter.
- **Persistence hook**: on mount, paint from localStorage cache immediately, then fetch
  `GET /filter-state` and reconcile; on change, debounce-write to localStorage and
  `PUT /filter-state` (server is source of truth).
- **`CommandCenterPage.tsx`**: hold the richer filter state; remove the "default to
  earthquake" fetch hack; wire load-more; one-way layer sync (derive allowed layers from
  selected categories); live-stream handling per §11.

## 11. Live stream behavior

Incoming WebSocket events are filtered client-side against the active filter. When
`sort = "recent"`, matching events auto-insert at the correct position. For other sorts,
matching events increment a "new events" badge that, when clicked, refetches the current
query. Non-matching events are ignored.

## 12. Globe layer sync (one-way)

Selected categories determine which layers may render: `earthquake` → quake dots/heat,
`weather` → weather layer. Excluding a category disables its layer toggle(s). Categories
without a layer have no globe representation yet (visible in list/detail/search only). The
user can still toggle an *allowed* layer off manually.

## 13. Security & correctness

- **Tenant isolation**: every events/search/saved-filters/filter-state/sources query is
  scoped to the caller's tenant (+ system tenant for public feeds where applicable).
  Cross-tenant/cross-user access returns 404. Add isolation tests (`tenant-isolation.mdc`).
- **RBAC**: saved-filters endpoints gated by `saved_filters.read`/`manage`; deny tests.
- **Audit**: `saved_filter.create|update|delete` write audit rows (`audit-log-coverage.mdc`).
- **No PII/secrets** in logs or error bodies.

## 14. Testing

- **Core**: query-builder tests for multi-value, ranges, magnitude-null exclusion, each
  sort (incl. nulls-last); distinct-sources query; saved-filters CRUD + user_filter_state
  upsert with tenant+user isolation.
- **API**: saved-filters CRUD + RBAC grant/deny + cross-tenant 404; filter-state upsert
  isolation; events/search param parsing + validation; auth response includes permissions.
- **Search/Meili**: filterable-attribute settings include `source`/`occurred_at`.
- **Frontend**: `pnpm exec tsc --noEmit`, `pnpm lint`, `pnpm build`; smokes — apply filter,
  save/apply/delete preset, reload persists from server, layer sync, live "new events" badge.

## 15. Migrations summary

- `0009_saved_filters.sql` — presets table.
- `0010_user_filter_state.sql` — per-user active filter.
- `0011_events_filter_sort_indexes.sql` — sort-supporting indexes.

All are new forward migrations; no shipped migration is edited (`api-sqlx-migrations.mdc`).

## 16. Risks / notes

- Sorting by impact/magnitude across many partitions can be costly; indexes in §5 mitigate,
  but very large tenants may need per-partition tuning later.
- Default = all categories means the globe shows quakes only while the list shows everything;
  acceptable until non-quake layers exist (and the event counter reflects the full list).
- Adding permissions to the session is a minor auth-contract change; the frontend
  `ApiError`/session parsing is updated in the same change set.
