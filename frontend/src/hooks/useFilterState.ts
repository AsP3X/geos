import { useCallback, useEffect, useRef, useState } from "react";
import {
  DEFAULT_FILTERS,
  type EventFilters,
  sanitizeFilters,
} from "@/components/filters/filters";
import { getFilterState, putFilterState } from "@/lib/filter-state-api";

const CACHE_KEY = "geos.filters.active";
const WRITE_DEBOUNCE_MS = 600;

function loadCache(): EventFilters | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    return raw ? sanitizeFilters(JSON.parse(raw)) : null;
  } catch {
    return null;
  }
}

function saveCache(filters: EventFilters): void {
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify(filters));
  } catch {
    // Ignore quota/availability errors; the server remains the source of truth.
  }
}

interface UseFilterStateOptions {
  enabled: boolean;
  getAccessToken: () => Promise<string | null>;
}

/**
 * Server-authoritative active-filter state with a localStorage fast-path. Paints
 * instantly from cache, reconciles against `GET /filter-state` on mount, and
 * debounce-writes changes to both cache and `PUT /filter-state`.
 */
export function useFilterState({ enabled, getAccessToken }: UseFilterStateOptions): {
  filters: EventFilters;
  setFilters: (filters: EventFilters) => void;
} {
  const [filters, setFiltersState] = useState<EventFilters>(
    () => loadCache() ?? { ...DEFAULT_FILTERS },
  );
  // Skip the server reconcile if the user has already changed the filter.
  const touchedRef = useRef(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => {
    if (!enabled) {
      return undefined;
    }
    let cancelled = false;
    void (async () => {
      const token = await getAccessToken();
      if (!token || cancelled) {
        return;
      }
      try {
        const server = await getFilterState(token);
        if (!cancelled && server && !touchedRef.current) {
          setFiltersState(server);
          saveCache(server);
        }
      } catch {
        // Keep the cached value on failure.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [enabled, getAccessToken]);

  useEffect(
    () => () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
      }
    },
    [],
  );

  const setFilters = useCallback(
    (next: EventFilters) => {
      touchedRef.current = true;
      setFiltersState(next);
      saveCache(next);
      if (timerRef.current) {
        clearTimeout(timerRef.current);
      }
      timerRef.current = setTimeout(() => {
        void (async () => {
          const token = await getAccessToken();
          if (!token) {
            return;
          }
          try {
            await putFilterState(token, next);
          } catch {
            // Cache already holds the value; retry on next change.
          }
        })();
      }, WRITE_DEBOUNCE_MS);
    },
    [getAccessToken],
  );

  return { filters, setFilters };
}
