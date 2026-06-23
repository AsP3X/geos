import { apiDelete, apiGet, apiPost, apiPut } from "@/lib/api-client";
import { type EventFilters, sanitizeFilters } from "@/components/filters/filters";

/** A stored preset as returned by the API (filters sanitized on read). */
export interface SavedFilter {
  id: string;
  name: string;
  filters: EventFilters;
  created_at: string;
  updated_at: string;
}

interface SavedFilterListResponse {
  items: SavedFilter[];
}

function normalize(saved: SavedFilter): SavedFilter {
  return { ...saved, filters: sanitizeFilters(saved.filters) };
}

export async function listSavedFilters(accessToken: string): Promise<SavedFilter[]> {
  const response = await apiGet<SavedFilterListResponse>("/api/v1/saved-filters", accessToken);
  return response.items.map(normalize);
}

export async function createSavedFilter(
  accessToken: string,
  name: string,
  filters: EventFilters,
): Promise<SavedFilter> {
  const saved = await apiPost<SavedFilter>(
    "/api/v1/saved-filters",
    { name, filters },
    accessToken,
  );
  return normalize(saved);
}

export async function updateSavedFilter(
  accessToken: string,
  id: string,
  name: string,
  filters: EventFilters,
): Promise<SavedFilter> {
  const saved = await apiPut<SavedFilter>(
    `/api/v1/saved-filters/${id}`,
    { name, filters },
    accessToken,
  );
  return normalize(saved);
}

export async function deleteSavedFilter(accessToken: string, id: string): Promise<void> {
  await apiDelete<void>(`/api/v1/saved-filters/${id}`, accessToken);
}
