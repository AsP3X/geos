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

export async function listEvents(
  accessToken: string,
  params?: { limit?: number; offset?: number },
): Promise<EventListResponse> {
  const search = new URLSearchParams();
  if (params?.limit !== undefined) {
    search.set("limit", String(params.limit));
  }
  if (params?.offset !== undefined) {
    search.set("offset", String(params.offset));
  }
  const query = search.toString();
  const path = query.length > 0 ? `/api/v1/events?${query}` : "/api/v1/events";
  return apiGet<EventListResponse>(path, accessToken);
}

export async function searchEvents(
  accessToken: string,
  query: string,
  limit = 20,
): Promise<SearchResults> {
  const params = new URLSearchParams({ q: query, limit: String(limit) });
  return apiGet<SearchResults>(`/api/v1/search?${params}`, accessToken);
}

export function streamUrl(accessToken: string): string {
  const params = new URLSearchParams({ token: accessToken });
  return `${streamBaseUrl()}/api/v1/stream?${params}`;
}
