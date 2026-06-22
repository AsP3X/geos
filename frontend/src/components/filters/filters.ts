import type { Event } from "@/types/event";

export interface EventFilters {
  category?: Event["category"];
  severity?: Event["severity"];
  minImpact: number;
}

/** Default filter state: everything visible. */
export const DEFAULT_FILTERS: EventFilters = {
  category: undefined,
  severity: undefined,
  minImpact: 0,
};

export function filtersAreActive(filters: EventFilters): boolean {
  return (
    filters.category !== undefined ||
    filters.severity !== undefined ||
    filters.minImpact > 0
  );
}

export const CATEGORIES: Event["category"][] = [
  "earthquake",
  "incident",
  "alert",
  "weather",
  "news",
  "conflict",
  "wildfire",
  "other",
];

export const SEVERITIES: Event["severity"][] = [
  "info",
  "low",
  "moderate",
  "high",
  "critical",
];
