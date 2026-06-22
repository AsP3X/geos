import { type FormEvent, lazy, Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { ChevronLeft, ChevronRight, Radio, SlidersHorizontal, X } from "lucide-react";
import { useAuth } from "@/hooks/useAuth";
import { EventDetail, EventList } from "@/components/events/EventPanels";
import { FilterPanel } from "@/components/filters/FilterPanel";
import {
  DEFAULT_FILTERS,
  type EventFilters,
  filtersAreActive,
} from "@/components/filters/filters";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useEventStream } from "@/hooks/useEventStream";
import { ApiError } from "@/lib/api-client";
import { listEvents, searchEvents } from "@/lib/events-api";
import type { Event } from "@/types/event";

const GlobeViewport = lazy(() =>
  import("@/components/globe/GlobeViewport").then((module) => ({
    default: module.GlobeViewport,
  })),
);

function mergeEvent(list: Event[], incoming: Event): Event[] {
  const index = list.findIndex((item) => item.id === incoming.id);
  if (index === -1) {
    return [incoming, ...list].sort(
      (a, b) => new Date(b.occurred_at).getTime() - new Date(a.occurred_at).getTime(),
    );
  }
  const next = [...list];
  next[index] = incoming;
  return next;
}

/** Floating card column: compact, viewport-bounded, scrolls internally.
 * Note: no `relative` here — these cards are placed with `absolute`, and a
 * second position utility would override it (Tailwind orders `.relative`
 * after `.absolute`), dropping the card back into normal flow. */
const FLOATING_CARD_CLASS =
  "glass-panel flex max-h-[calc(100dvh-2rem)] flex-col overflow-hidden rounded-[1.4rem]";

/** Shared header label styling for floating cards. */
const CARD_LABEL_CLASS =
  "text-[11px] font-semibold uppercase tracking-[0.16em] text-foreground/55";

/** Collapse/expand control inside card headers. */
const CARD_TOGGLE_CLASS =
  "rounded-full p-1.5 text-foreground/50 transition-colors hover:bg-white/10 hover:text-foreground";

export function CommandCenterPage() {
  const { session, logout, getAccessToken } = useAuth();
  const [events, setEvents] = useState<Event[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [accessToken, setAccessToken] = useState<string | null>(null);
  const [leftOpen, setLeftOpen] = useState(true);
  const [rightOpen, setRightOpen] = useState(true);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [filters, setFilters] = useState<EventFilters>(DEFAULT_FILTERS);

  const selectedEvent = useMemo(
    () => events.find((event) => event.id === selectedId) ?? null,
    [events, selectedId],
  );

  const loadEvents = useCallback(async () => {
    setError(null);
    const token = await getAccessToken();
    if (!token) {
      return;
    }
    setAccessToken(token);
    try {
      const response = await listEvents(token, {
        limit: 100,
        category: filters.category,
        severity: filters.severity,
        minImpact: filters.minImpact,
      });
      setEvents(response.items);
      setSelectedId((current) => current ?? response.items[0]?.id ?? null);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Failed to load events");
    } finally {
      setLoading(false);
    }
  }, [getAccessToken, filters]);

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- data fetch on mount
    void loadEvents();
  }, [loadEvents]);

  const onStreamEvent = useCallback((event: Event) => {
    setEvents((current) => mergeEvent(current, event));
  }, []);

  const { connected } = useEventStream({
    enabled: Boolean(accessToken),
    accessToken,
    onEvent: onStreamEvent,
  });

  async function onSearch(event: FormEvent) {
    event.preventDefault();
    const q = searchQuery.trim();
    if (!q) {
      await loadEvents();
      return;
    }
    const token = await getAccessToken();
    if (!token) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const results = await searchEvents(token, q, {
        category: filters.category,
        severity: filters.severity,
        minImpact: filters.minImpact,
      });
      const mapped: Event[] = results.hits.map((hit) => ({
        id: hit.id,
        tenant_id: hit.tenant_id,
        source: hit.source,
        source_event_id: hit.id,
        category: hit.category,
        severity: hit.severity,
        impact_score: hit.impact_score,
        title: hit.title ?? undefined,
        summary: hit.summary ?? undefined,
        place_name: hit.place_name ?? undefined,
        location: { lat: hit.lat, lon: hit.lon },
        occurred_at: hit.occurred_at,
        ingested_at: hit.occurred_at,
        status: "active",
        verification_status: "unverified",
        confidence: 0,
        tags: [],
        raw: {},
      }));
      setEvents(mapped);
      if (mapped.length > 0) {
        setSelectedId(mapped[0].id);
      }
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Search failed");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="fixed inset-0 overflow-hidden bg-background">
      <div className="absolute inset-0 z-0">
        <Suspense
          fallback={
            <div className="flex size-full items-center justify-center bg-[#0a0c10] text-sm text-foreground/50">
              Loading globe…
            </div>
          }
        >
          <GlobeViewport events={events} selectedId={selectedId} onSelect={setSelectedId} />
        </Suspense>
      </div>

      {/* Floating centered top bar pill. */}
      <header className="glass-panel absolute left-1/2 top-4 z-30 flex h-12 max-w-[calc(100vw-2rem)] -translate-x-1/2 items-center gap-2 rounded-full pl-5 pr-2">
        <h1 className="shrink-0 text-base font-semibold tracking-tight text-primary">Geos</h1>
        <span className="mx-1 hidden h-5 w-px shrink-0 bg-white/15 sm:block" aria-hidden />
        <form className="flex min-w-0 flex-1 items-center gap-2" onSubmit={onSearch}>
          <Input
            className="h-8 w-full min-w-0 rounded-full border-white/10 bg-white/5 text-sm placeholder:text-foreground/40 focus-visible:ring-primary/40 sm:w-72"
            placeholder="Search events…"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
          <Button
            type="submit"
            size="sm"
            variant="ghost"
            className="shrink-0 rounded-full hover:bg-white/10"
          >
            Search
          </Button>
        </form>
        <span className="mx-1 hidden h-5 w-px shrink-0 bg-white/15 sm:block" aria-hidden />
        <div className="flex shrink-0 items-center gap-3 pr-1 text-xs text-foreground/60">
          <span className="inline-flex items-center gap-1.5">
            <Radio className={connected ? "text-primary" : "text-foreground/30"} size={14} />
            <span className="hidden md:inline">{connected ? "Live" : "Offline"}</span>
          </span>
          <span className="hidden text-foreground/45 lg:inline">{session?.tenantSlug}</span>
          <Button
            size="sm"
            variant="ghost"
            className="shrink-0 rounded-full hover:bg-white/10"
            onClick={logout}
          >
            Sign out
          </Button>
        </div>
      </header>

      {error ? (
        <div className="glass-panel absolute left-1/2 top-[4.75rem] z-30 -translate-x-1/2 rounded-full !border-destructive/40 px-4 py-1.5 text-sm text-destructive">
          {error}
        </div>
      ) : null}

      {/* Floating left events card (collapsible). */}
      {leftOpen ? (
        <aside
          className={`absolute left-4 top-4 z-20 w-[min(18rem,calc(50vw-1.5rem))] ${FLOATING_CARD_CLASS}`}
        >
          <div className="flex items-center justify-between border-b border-white/10 px-4 py-3">
            <span className={CARD_LABEL_CLASS}>Events</span>
            <button
              type="button"
              aria-label="Collapse events panel"
              className={CARD_TOGGLE_CLASS}
              onClick={() => setLeftOpen(false)}
            >
              <ChevronLeft size={16} />
            </button>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">
            {loading ? (
              <p className="p-4 text-sm text-foreground/50">Loading…</p>
            ) : (
              <EventList events={events} selectedId={selectedId} onSelect={setSelectedId} />
            )}
          </div>
        </aside>
      ) : (
        <button
          type="button"
          aria-label="Open events panel"
          className="glass-panel absolute left-4 top-4 z-20 flex size-11 items-center justify-center rounded-2xl text-foreground/60 transition-colors hover:text-foreground"
          onClick={() => setLeftOpen(true)}
        >
          <ChevronRight size={18} />
        </button>
      )}

      {/* Floating right detail card (collapsible). */}
      {rightOpen ? (
        <aside
          className={`absolute right-4 top-4 z-20 w-[min(20rem,calc(50vw-1.5rem))] ${FLOATING_CARD_CLASS}`}
        >
          <div className="flex items-center justify-between border-b border-white/10 px-4 py-3">
            <span className={CARD_LABEL_CLASS}>Detail</span>
            <button
              type="button"
              aria-label="Collapse detail panel"
              className={CARD_TOGGLE_CLASS}
              onClick={() => setRightOpen(false)}
            >
              <ChevronRight size={16} />
            </button>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">
            <EventDetail event={selectedEvent} />
          </div>
        </aside>
      ) : (
        <button
          type="button"
          aria-label="Open detail panel"
          className="glass-panel absolute right-4 top-4 z-20 flex size-11 items-center justify-center rounded-2xl text-foreground/60 transition-colors hover:text-foreground"
          onClick={() => setRightOpen(true)}
        >
          <ChevronLeft size={18} />
        </button>
      )}

      {/* Floating filters panel (bottom-right, collapsible). */}
      {filtersOpen ? (
        <aside
          className={`absolute bottom-4 right-4 z-20 w-[min(17rem,calc(100vw-2rem))] ${FLOATING_CARD_CLASS}`}
        >
          <div className="flex items-center justify-between border-b border-white/10 px-4 py-3">
            <span className={CARD_LABEL_CLASS}>Filters</span>
            <button
              type="button"
              aria-label="Close filters panel"
              className={CARD_TOGGLE_CLASS}
              onClick={() => setFiltersOpen(false)}
            >
              <X size={16} />
            </button>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">
            <FilterPanel filters={filters} onChange={setFilters} />
          </div>
        </aside>
      ) : (
        <button
          type="button"
          aria-label="Open filters panel"
          className="glass-panel absolute bottom-4 right-4 z-20 flex size-11 items-center justify-center rounded-2xl text-foreground/60 transition-colors hover:text-foreground"
          onClick={() => setFiltersOpen(true)}
        >
          <SlidersHorizontal size={18} />
          {filtersAreActive(filters) ? (
            <span className="absolute right-2 top-2 size-2 rounded-full bg-primary" aria-hidden />
          ) : null}
        </button>
      )}
    </div>
  );
}
