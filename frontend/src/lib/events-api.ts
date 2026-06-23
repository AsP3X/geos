import type { Event } from "@/types/event";
import { apiGet } from "@/lib/api-client";
import { type EventFilters, normalizeFiltersForQuery, resolveTimeRange } from "@/components/filters/filters";
import { streamBaseUrl } from "@/lib/env";

export interface EventListResponse {
  items: Event[];
  total: number;
  limit: number;
  offset: number;
}

export interface EventMapPoint {
  id: string;
  category: Event["category"];
  severity: Event["severity"];
  impact_score: number;
  magnitude?: number | null;
  lat: number;
  lon: number;
  occurred_at: string;
}

export interface EventMapResponse {
  points: EventMapPoint[];
  total: number;
  limit: number;
}

/** Points fetched per globe map batch (keeps transfers small). */
export const GLOBE_MAP_BATCH_SIZE = 2_500;

/** Hard cap on points accumulated on the globe across all batches. */
export const GLOBE_MAP_MAX_POINTS = 100_000;

/** Pause between batches so the main thread can paint (~2 frames). */
export const GLOBE_MAP_BATCH_PAUSE_MS = 32;

export interface SearchResults {
  query: string;
  hits: EventSearchHit[];
  limit: number;
  offset: number;
  estimated_total: number;
}

export interface EventSearchHit {
  id: string;
  tenant_id: string;
  source: string;
  title?: string | null;
  summary?: string | null;
  place_name?: string | null;
  category: Event["category"];
  severity: Event["severity"];
  impact_score: number;
  occurred_at: string;
  lat: number;
  lon: number;
}

export interface StreamEnvelope {
  kind: "event.upsert";
  event: Event;
}

/** Serialize the shared filter into events/search query params. */
function appendFilterParams(
  search: URLSearchParams,
  filters: EventFilters,
  availableSources: string[] = [],
): void {
  const queryFilters = normalizeFiltersForQuery(filters, availableSources);
  if (queryFilters.categories.length > 0) {
    search.set("category", queryFilters.categories.join(","));
  }
  if (queryFilters.severities.length > 0) {
    search.set("severity", queryFilters.severities.join(","));
  }
  if (queryFilters.sources.length > 0) {
    search.set("source", queryFilters.sources.join(","));
  }
  if (queryFilters.impactMin > 0) {
    search.set("impact_min", String(queryFilters.impactMin));
  }
  if (queryFilters.impactMax < 100) {
    search.set("impact_max", String(queryFilters.impactMax));
  }
  if (queryFilters.magnitudeMin !== null) {
    search.set("min_magnitude", String(queryFilters.magnitudeMin));
  }
  if (queryFilters.magnitudeMax !== null) {
    search.set("max_magnitude", String(queryFilters.magnitudeMax));
  }
  search.set("sort", queryFilters.sort);
  const { occurredAfter, occurredBefore } = resolveTimeRange(queryFilters);
  if (occurredAfter) {
    search.set("occurred_after", occurredAfter);
  }
  if (occurredBefore) {
    search.set("occurred_before", occurredBefore);
  }
}

export interface ListEventsParams {
  filters: EventFilters;
  availableSources?: string[];
  limit?: number;
  offset?: number;
}

export async function listEvents(
  accessToken: string,
  params: ListEventsParams,
): Promise<EventListResponse> {
  const search = new URLSearchParams();
  if (params.limit !== undefined) {
    search.set("limit", String(params.limit));
  }
  if (params.offset !== undefined) {
    search.set("offset", String(params.offset));
  }
  appendFilterParams(search, params.filters, params.availableSources ?? []);
  return apiGet<EventListResponse>(`/api/v1/events?${search}`, accessToken);
}

/** Compact map coordinates for globe heat/dots (same filters, paginated). */
export async function listEventMapPoints(
  accessToken: string,
  params: ListEventsParams & { limit?: number },
): Promise<EventMapResponse> {
  const search = new URLSearchParams();
  search.set("limit", String(params.limit ?? GLOBE_MAP_BATCH_SIZE));
  if (params.offset !== undefined) {
    search.set("offset", String(params.offset));
  }
  appendFilterParams(search, params.filters, params.availableSources ?? []);
  return apiGet<EventMapResponse>(`/api/v1/events/map?${search}`, accessToken);
}

/** Build a minimal canonical event from a map point for globe layers. */
export function mapPointToEvent(point: EventMapPoint): Event {
  return {
    id: point.id,
    tenant_id: "",
    source: "",
    source_event_id: point.id,
    category: point.category,
    severity: point.severity,
    impact_score: point.impact_score,
    magnitude: point.magnitude ?? undefined,
    location: { lat: point.lat, lon: point.lon },
    occurred_at: point.occurred_at,
    ingested_at: point.occurred_at,
    status: "active",
    verification_status: "unverified",
    confidence: 0,
    tags: [],
    raw: {},
  };
}

/** Fetch one full canonical event by id (e.g. after clicking a globe dot). */
export async function getEvent(accessToken: string, id: string): Promise<Event> {
  return apiGet<Event>(`/api/v1/events/${id}`, accessToken);
}

export interface EventSourcesResponse {
  sources: string[];
}

/** Distinct source keys present in the tenant's data (drives the source filter). */
export async function listEventSources(accessToken: string): Promise<string[]> {
  const response = await apiGet<EventSourcesResponse>("/api/v1/events/sources", accessToken);
  return response.sources;
}

export async function searchEvents(
  accessToken: string,
  query: string,
  filters: EventFilters,
  availableSources: string[] = [],
  limit = 50,
): Promise<SearchResults> {
  const params = new URLSearchParams({ q: query, limit: String(limit) });
  appendFilterParams(params, filters, availableSources);
  return apiGet<SearchResults>(`/api/v1/search?${params}`, accessToken);
}

export function streamUrl(accessToken: string): string {
  const params = new URLSearchParams({ token: accessToken });
  return `${streamBaseUrl()}/api/v1/stream?${params}`;
}
