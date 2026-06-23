import { apiGet, apiPut } from "@/lib/api-client";
import { type EventFilters, sanitizeFilters } from "@/components/filters/filters";

interface FilterStateResponse {
  filters: EventFilters | null;
}

/** Read the caller's server-authoritative active filter (null if never saved). */
export async function getFilterState(accessToken: string): Promise<EventFilters | null> {
  const response = await apiGet<FilterStateResponse>("/api/v1/filter-state", accessToken);
  return response.filters ? sanitizeFilters(response.filters) : null;
}

/** Upsert the caller's active filter (server is the source of truth). */
export async function putFilterState(
  accessToken: string,
  filters: EventFilters,
): Promise<void> {
  await apiPut<void>("/api/v1/filter-state", { filters }, accessToken);
}
