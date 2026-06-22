import type { Event } from "@/types/event";
import { apiGet } from "@/lib/api-client";
import { streamBaseUrl } from "@/lib/env";

export interface EventListResponse {
  items: Event[];
  limit: number;
  offset: number;
}

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

export interface EventListParams {
  limit?: number;
  offset?: number;
  category?: Event["category"];
  severity?: Event["severity"];
  minImpact?: number;
}

export async function listEvents(
  accessToken: string,
  params?: EventListParams,
): Promise<EventListResponse> {
  const search = new URLSearchParams();
  if (params?.limit !== undefined) {
    search.set("limit", String(params.limit));
  }
  if (params?.offset !== undefined) {
    search.set("offset", String(params.offset));
  }
  if (params?.category) {
    search.set("category", params.category);
  }
  if (params?.severity) {
    search.set("severity", params.severity);
  }
  if (params?.minImpact !== undefined && params.minImpact > 0) {
    search.set("min_impact", String(params.minImpact));
  }
  const query = search.toString();
  const path = query.length > 0 ? `/api/v1/events?${query}` : "/api/v1/events";
  return apiGet<EventListResponse>(path, accessToken);
}

export interface SearchFilterParams {
  category?: Event["category"];
  severity?: Event["severity"];
  minImpact?: number;
}

export async function searchEvents(
  accessToken: string,
  query: string,
  filters?: SearchFilterParams,
  limit = 20,
): Promise<SearchResults> {
  const params = new URLSearchParams({ q: query, limit: String(limit) });
  if (filters?.category) {
    params.set("category", filters.category);
  }
  if (filters?.severity) {
    params.set("severity", filters.severity);
  }
  if (filters?.minImpact !== undefined && filters.minImpact > 0) {
    params.set("min_impact", String(filters.minImpact));
  }
  return apiGet<SearchResults>(`/api/v1/search?${params}`, accessToken);
}

export function streamUrl(accessToken: string): string {
  const params = new URLSearchParams({ token: accessToken });
  return `${streamBaseUrl()}/api/v1/stream?${params}`;
}
