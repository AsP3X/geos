import type { Event } from "@/types/event";

export type Category = Event["category"];
export type Severity = Event["severity"];

export type TimeRangePreset = "1h" | "24h" | "7d" | "30d" | "all" | "custom";
export type EventSort = "recent" | "impact_desc" | "magnitude_desc";

/** Current stored-filter schema version (sanitizer resets unknown versions). */
export const FILTERS_VERSION = 1;

/**
 * The single filter shape that drives the event query, presets, persistence,
 * and (where supported) search. Wire format matches the backend
 * `SavedFilterPayload` (camelCase), so it round-trips without translation.
 */
export interface EventFilters {
  version: number;
  categories: Category[];
  severities: Severity[];
  sources: string[];
  /** Impact lower bound, 0–100. */
  impactMin: number;
  /** Impact upper bound, 0–100. */
  impactMax: number;
  /** Magnitude lower bound; null = no lower bound. */
  magnitudeMin: number | null;
  /** Magnitude upper bound; null = no upper bound. */
  magnitudeMax: number | null;
  timeRange: TimeRangePreset;
  /** Custom-range start (ISO-8601) when timeRange = "custom". */
  from: string | null;
  /** Custom-range end (ISO-8601) when timeRange = "custom". */
  to: string | null;
  sort: EventSort;
}

/** Default filter state: all data within the last 30 days, recent-first. */
export const DEFAULT_FILTERS: EventFilters = {
  version: FILTERS_VERSION,
  categories: [],
  severities: [],
  sources: [],
  impactMin: 0,
  impactMax: 100,
  magnitudeMin: null,
  magnitudeMax: null,
  timeRange: "30d",
  from: null,
  to: null,
  sort: "recent",
};

export const CATEGORIES: Category[] = [
  "earthquake",
  "incident",
  "alert",
  "weather",
  "news",
  "conflict",
  "wildfire",
  "other",
];

export const SEVERITIES: Severity[] = ["info", "low", "moderate", "high", "critical"];

export const TIME_RANGES: { value: TimeRangePreset; label: string }[] = [
  { value: "1h", label: "1h" },
  { value: "24h", label: "24h" },
  { value: "7d", label: "7d" },
  { value: "30d", label: "30d" },
  { value: "all", label: "All" },
  { value: "custom", label: "Custom" },
];

export const SORT_OPTIONS: { value: EventSort; label: string }[] = [
  { value: "recent", label: "Most recent" },
  { value: "impact_desc", label: "Highest impact" },
  { value: "magnitude_desc", label: "Highest magnitude" },
];

const TIME_RANGE_VALUES = new Set<TimeRangePreset>(TIME_RANGES.map((t) => t.value));
const SORT_VALUES = new Set<EventSort>(SORT_OPTIONS.map((s) => s.value));
const CATEGORY_SET = new Set<Category>(CATEGORIES);
const SEVERITY_SET = new Set<Severity>(SEVERITIES);

function clampImpact(value: unknown, fallback: number): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return fallback;
  }
  return Math.min(100, Math.max(0, Math.round(value)));
}

function sanitizeMagnitude(value: unknown): number | null {
  if (value === null || value === undefined) {
    return null;
  }
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return null;
  }
  return value;
}

function sanitizeIso(value: unknown): string | null {
  if (typeof value !== "string") {
    return null;
  }
  const ms = Date.parse(value);
  return Number.isNaN(ms) ? null : new Date(ms).toISOString();
}

/**
 * Coerce arbitrary stored/loaded JSON into a valid `EventFilters`. Unknown
 * values fall back to defaults; an unrecognized version resets entirely
 * (forward-compatible, versioned migration of stored JSON).
 */
export function sanitizeFilters(raw: unknown): EventFilters {
  if (!raw || typeof raw !== "object") {
    return { ...DEFAULT_FILTERS };
  }
  const input = raw as Record<string, unknown>;

  // Unknown future schema → reset to defaults rather than trust foreign shape.
  if (typeof input.version === "number" && input.version > FILTERS_VERSION) {
    return { ...DEFAULT_FILTERS };
  }

  const categories = Array.isArray(input.categories)
    ? (input.categories.filter((c): c is Category => CATEGORY_SET.has(c as Category)) as Category[])
    : [];
  const severities = Array.isArray(input.severities)
    ? (input.severities.filter((s): s is Severity => SEVERITY_SET.has(s as Severity)) as Severity[])
    : [];
  const sources = Array.isArray(input.sources)
    ? input.sources.filter((s): s is string => typeof s === "string")
    : [];

  let impactMin = clampImpact(input.impactMin, 0);
  let impactMax = clampImpact(input.impactMax, 100);
  if (impactMin > impactMax) {
    [impactMin, impactMax] = [impactMax, impactMin];
  }

  let magnitudeMin = sanitizeMagnitude(input.magnitudeMin);
  let magnitudeMax = sanitizeMagnitude(input.magnitudeMax);
  if (magnitudeMin !== null && magnitudeMax !== null && magnitudeMin > magnitudeMax) {
    [magnitudeMin, magnitudeMax] = [magnitudeMax, magnitudeMin];
  }

  const timeRange = TIME_RANGE_VALUES.has(input.timeRange as TimeRangePreset)
    ? (input.timeRange as TimeRangePreset)
    : DEFAULT_FILTERS.timeRange;
  const sort = SORT_VALUES.has(input.sort as EventSort)
    ? (input.sort as EventSort)
    : DEFAULT_FILTERS.sort;

  return {
    version: FILTERS_VERSION,
    categories,
    severities,
    sources,
    impactMin,
    impactMax,
    magnitudeMin,
    magnitudeMax,
    timeRange,
    from: sanitizeIso(input.from),
    to: sanitizeIso(input.to),
    sort,
  };
}

/** Whether any dimension deviates from the defaults (drives the active badge). */
export function filtersAreActive(filters: EventFilters): boolean {
  return (
    filters.categories.length > 0 ||
    filters.severities.length > 0 ||
    filters.sources.length > 0 ||
    filters.impactMin > 0 ||
    filters.impactMax < 100 ||
    filters.magnitudeMin !== null ||
    filters.magnitudeMax !== null ||
    filters.timeRange !== DEFAULT_FILTERS.timeRange ||
    filters.sort !== DEFAULT_FILTERS.sort
  );
}

/**
 * Collapse a full multi-select back to "no constraint" (empty array). Selecting
 * every chip in a dimension is equivalent to selecting none and avoids sending
 * redundant `ANY(...)` clauses that behave differently under pagination.
 */
export function normalizeFiltersForQuery(
  filters: EventFilters,
  availableSources: string[] = [],
): EventFilters {
  const categories =
    filters.categories.length === CATEGORIES.length ? [] : filters.categories;
  const severities =
    filters.severities.length === SEVERITIES.length ? [] : filters.severities;
  const sources =
    availableSources.length > 0 &&
    filters.sources.length === availableSources.length &&
    availableSources.every((source) => filters.sources.includes(source))
      ? []
      : filters.sources;

  if (
    categories.length === filters.categories.length &&
    severities.length === filters.severities.length &&
    sources.length === filters.sources.length
  ) {
    return filters;
  }
  return { ...filters, categories, severities, sources };
}

/**
 * Evaluate a single event against the active filter, client-side. Used to decide
 * whether a live-stream event belongs in the current view. Mirrors the
 * server-side predicate (magnitude bound implies magnitude present).
 */
export function eventMatchesFilters(event: Event, filters: EventFilters): boolean {
  if (filters.categories.length > 0 && !filters.categories.includes(event.category)) {
    return false;
  }
  if (filters.severities.length > 0 && !filters.severities.includes(event.severity)) {
    return false;
  }
  if (filters.sources.length > 0 && !filters.sources.includes(event.source)) {
    return false;
  }
  if (event.impact_score < filters.impactMin || event.impact_score > filters.impactMax) {
    return false;
  }
  if (filters.magnitudeMin !== null || filters.magnitudeMax !== null) {
    if (event.magnitude == null) {
      return false;
    }
    if (filters.magnitudeMin !== null && event.magnitude < filters.magnitudeMin) {
      return false;
    }
    if (filters.magnitudeMax !== null && event.magnitude > filters.magnitudeMax) {
      return false;
    }
  }
  const { occurredAfter, occurredBefore } = resolveTimeRange(filters);
  const occurredMs = Date.parse(event.occurred_at);
  if (!Number.isNaN(occurredMs)) {
    if (occurredAfter && occurredMs < Date.parse(occurredAfter)) {
      return false;
    }
    if (occurredBefore && occurredMs > Date.parse(occurredBefore)) {
      return false;
    }
  }
  return true;
}

const PRESET_DURATIONS_MS: Partial<Record<TimeRangePreset, number>> = {
  "1h": 60 * 60 * 1000,
  "24h": 24 * 60 * 60 * 1000,
  "7d": 7 * 24 * 60 * 60 * 1000,
  "30d": 30 * 24 * 60 * 60 * 1000,
};

/**
 * Resolve the time range to absolute ISO bounds for the query. `all` yields no
 * bounds; `custom` uses the stored from/to; presets are relative to now.
 */
export function resolveTimeRange(filters: EventFilters): {
  occurredAfter?: string;
  occurredBefore?: string;
} {
  if (filters.timeRange === "all") {
    return {};
  }
  if (filters.timeRange === "custom") {
    const result: { occurredAfter?: string; occurredBefore?: string } = {};
    if (filters.from) {
      result.occurredAfter = filters.from;
    }
    if (filters.to) {
      result.occurredBefore = filters.to;
    }
    return result;
  }
  const duration = PRESET_DURATIONS_MS[filters.timeRange];
  if (duration === undefined) {
    return {};
  }
  return { occurredAfter: new Date(Date.now() - duration).toISOString() };
}
