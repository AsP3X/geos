import { type FormEvent, lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Activity, ChevronLeft, ChevronRight, Radio, SlidersHorizontal, X } from "lucide-react";
import { useAuth } from "@/hooks/useAuth";
import { EventDetail, EventList } from "@/components/events/EventPanels";
import { IngestionStatus } from "@/components/connectors/IngestionStatus";
import { FilterPanel } from "@/components/filters/FilterPanel";
import { eventMatchesFilters, filtersAreActive, type EventFilters } from "@/components/filters/filters";
import { LayersRail } from "@/components/globe/LayersRail";
import { DEFAULT_LAYERS, isQuake, type GlobeLayers } from "@/components/globe/layers";
import type { GlobeCluster } from "@/components/globe/cesium/clusters";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useConnectorStatus } from "@/hooks/useConnectorStatus";
import { useEventStream } from "@/hooks/useEventStream";
import { useFilterState } from "@/hooks/useFilterState";
import { useSavedFilters } from "@/hooks/useSavedFilters";
import { ApiError } from "@/lib/api-client";
import { sessionHasPermission } from "@/lib/auth-storage";
import {
  GLOBE_MAP_BATCH_PAUSE_MS,
  GLOBE_MAP_BATCH_SIZE,
  GLOBE_MAP_MAX_POINTS,
  getEvent,
  type GlobeBBox,
  listEvents,
  listEventMapPoints,
  listEventSources,
  mapPointToEvent,
  searchEvents,
} from "@/lib/events-api";
import type { Event } from "@/types/event";

const PAGE_SIZE = 200;

const GlobeViewport = lazy(() =>
  import("@/components/globe/cesium/CesiumGlobeViewport").then((module) => ({
    default: module.CesiumGlobeViewport,
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

/** Quake-only merge for the globe layers (dots + heat). */
function mergeGlobeEvent(list: Event[], incoming: Event): Event[] {
  if (!isQuake(incoming)) {
    return list;
  }
  return mergeEvent(list, incoming);
}

function quakesFrom(items: Event[]): Event[] {
  return items.filter(isQuake);
}

function appendGlobeBatch(current: Event[], batch: Event[]): Event[] {
  if (batch.length === 0) {
    return current;
  }
  const seen = new Set(current.map((event) => event.id));
  const additions = batch.filter((event) => !seen.has(event.id));
  return additions.length > 0 ? [...current, ...additions] : current;
}

function pause(ms: number): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, ms);
  });
}

/** Globe layers only visualize quakes — narrow the map query accordingly. */
function filtersForGlobeMap(filters: EventFilters): EventFilters | null {
  if (filters.categories.length > 0 && !filters.categories.includes("earthquake")) {
    return null;
  }
  // Recent-first batches: newest dots appear immediately, older timeframe fills in.
  return { ...filters, categories: ["earthquake"], sort: "recent" };
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

/** Shared floating action button styling (bottom-right cluster). */
const FAB_CLASS =
  "glass-panel relative flex size-11 items-center justify-center rounded-2xl text-foreground/60 transition-colors hover:text-foreground";

/** Sit controls to the left of the open filters panel. */
const RIGHT_OF_FILTERS_PANEL = "right-[calc(1rem+min(17rem,calc(100vw-2rem))+0.75rem)]";

/** Sit the layers rail just right of the open events panel (matches its width). */
const RIGHT_OF_EVENTS_PANEL = "left-[calc(1rem+min(18rem,calc(50vw-1.5rem))+0.75rem)]";

export function CommandCenterPage() {
  const { session, logout, getAccessToken } = useAuth();
  const [events, setEvents] = useState<Event[]>([]);
  /** Quake markers/heat — always live + filter-synced, independent of list sort/pagination. */
  const [globeEvents, setGlobeEvents] = useState<Event[]>([]);
  const [globeTotal, setGlobeTotal] = useState(0);
  const [globeLoading, setGlobeLoading] = useState(false);
  const [globeLoadingMore, setGlobeLoadingMore] = useState(false);
  /** Bumped on filter change so globe layers remount; not on each batch append. */
  const [globeEpoch, setGlobeEpoch] = useState(0);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [committedQuery, setCommittedQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [totalEvents, setTotalEvents] = useState(0);
  const [newEventsCount, setNewEventsCount] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [accessToken, setAccessToken] = useState<string | null>(null);
  const [leftOpen, setLeftOpen] = useState(true);
  const [rightOpen, setRightOpen] = useState(true);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [ingestionOpen, setIngestionOpen] = useState(false);
  const [layers, setLayers] = useState<GlobeLayers>(DEFAULT_LAYERS);
  const [sources, setSources] = useState<string[]>([]);
  const queryGenerationRef = useRef(0);
  // Globe loading runs on its own generation so viewport-driven refetches don't
  // cancel the sidebar list query. `globeBboxRef` holds the latest camera bounds
  // (null = whole-globe/global load).
  const globeGenerationRef = useRef(0);
  const globeBboxRef = useRef<GlobeBBox | null>(null);

  const enabled = Boolean(session);
  const { filters, setFilters } = useFilterState({ enabled, getAccessToken });
  const canManagePresets = sessionHasPermission(session, "saved_filters.manage");
  const canReadPresets = sessionHasPermission(session, "saved_filters.read");
  const {
    presets,
    save: savePreset,
    update: updatePreset,
    remove: removePreset,
  } = useSavedFilters({ enabled: enabled && canReadPresets, getAccessToken });

  const {
    connectors,
    loading: connectorsLoading,
    error: connectorsError,
    updatedAt: connectorsUpdatedAt,
    deltas: connectorDeltas,
  } = useConnectorStatus({
    enabled,
    getAccessToken,
    intervalMs: ingestionOpen ? 250 : 10_000,
  });

  // Full detail for a globe-clicked dot that isn't in the paginated list.
  const [selectedDetail, setSelectedDetail] = useState<Event | null>(null);
  // Members of a clicked cluster, shown in the left list for disambiguation.
  const [clusterFocus, setClusterFocus] = useState<{ events: Event[]; count: number } | null>(null);

  const handleSelectCluster = useCallback((cluster: GlobeCluster) => {
    setClusterFocus({ events: cluster.members, count: cluster.count });
    setLeftOpen(true);
    setSelectedId(cluster.members[0]?.id ?? null);
  }, []);

  const selectedEvent = useMemo(() => {
    if (!selectedId) {
      return null;
    }
    return (
      events.find((event) => event.id === selectedId) ??
      (selectedDetail?.id === selectedId ? selectedDetail : null) ??
      globeEvents.find((event) => event.id === selectedId) ??
      null
    );
  }, [events, globeEvents, selectedDetail, selectedId]);

  // Hydrate full event detail (title/summary/place) for globe-only selections.
  useEffect(() => {
    if (!selectedId || events.some((event) => event.id === selectedId)) {
      return;
    }
    if (selectedDetail?.id === selectedId) {
      return;
    }
    let cancelled = false;
    void (async () => {
      const token = await getAccessToken();
      if (!token || cancelled) {
        return;
      }
      try {
        const full = await getEvent(token, selectedId);
        if (!cancelled) {
          setSelectedDetail(full);
        }
      } catch {
        // Non-fatal: fall back to the compact globe point data.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [selectedId, events, selectedDetail, getAccessToken]);

  const searchHitToEvent = useCallback(
    (hit: Awaited<ReturnType<typeof searchEvents>>["hits"][number]): Event => ({
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
    }),
    [],
  );

  // Fetch matching quake coordinates in batches (recent first) for globe layers,
  // scoped to the current viewport `bbox` when zoomed in (null = global load).
  // The first batch replaces the previous set so dots from the prior view stay
  // visible until new data arrives (no flicker); later batches append.
  const loadGlobeData = useCallback(
    async (generation: number, bbox: GlobeBBox | null) => {
      if (committedQuery) {
        return;
      }
      const globeFilters = filtersForGlobeMap(filters);
      if (!globeFilters) {
        setGlobeEvents([]);
        setGlobeTotal(0);
        setGlobeLoading(false);
        setGlobeLoadingMore(false);
        return;
      }
      const token = await getAccessToken();
      if (!token || generation !== globeGenerationRef.current) {
        return;
      }
      setGlobeLoading(true);
      setGlobeLoadingMore(false);

      // Fetch the grand total in parallel (COUNT(*) can be slow) so the dots
      // stream in as each data batch arrives instead of waiting on the count.
      void (async () => {
        try {
          const countResponse = await listEventMapPoints(token, {
            filters: globeFilters,
            availableSources: sources,
            limit: 1,
            offset: 0,
            bbox,
            withCount: true,
          });
          if (generation === globeGenerationRef.current) {
            setGlobeTotal(countResponse.total);
          }
        } catch {
          // Non-fatal: the status pill just omits the grand total.
        }
      })();

      let offset = 0;

      try {
        while (generation === globeGenerationRef.current && offset < GLOBE_MAP_MAX_POINTS) {
          const response = await listEventMapPoints(token, {
            filters: globeFilters,
            availableSources: sources,
            limit: GLOBE_MAP_BATCH_SIZE,
            offset,
            bbox,
            withCount: false,
          });
          if (generation !== globeGenerationRef.current) {
            return;
          }

          const batch = response.points.map(mapPointToEvent);

          if (offset === 0) {
            setGlobeLoading(false);
            // Replace the previous view's points (viewport changed or new filter).
            setGlobeEvents(batch);
          } else {
            setGlobeEvents((current) => appendGlobeBatch(current, batch));
          }

          offset += batch.length;

          // Drive pagination by page fullness (no dependency on the count): a
          // short page means we reached the end of the matching set.
          const reachedCap = offset >= GLOBE_MAP_MAX_POINTS;
          const shortPage = batch.length < GLOBE_MAP_BATCH_SIZE;
          if (batch.length === 0 || shortPage || reachedCap) {
            break;
          }

          setGlobeLoadingMore(true);
          await pause(GLOBE_MAP_BATCH_PAUSE_MS);
        }
      } catch {
        if (generation !== globeGenerationRef.current) {
          return;
        }
        // Non-fatal: sidebar list still works; partial globe data may remain.
      } finally {
        if (generation === globeGenerationRef.current) {
          setGlobeLoading(false);
          setGlobeLoadingMore(false);
        }
      }
    },
    [committedQuery, filters, getAccessToken, sources],
  );

  // Camera settled: (re)load the globe scoped to the new viewport bounds.
  const handleViewportChange = useCallback(
    (bbox: GlobeBBox | null) => {
      if (committedQuery) {
        return;
      }
      globeBboxRef.current = bbox;
      const generation = ++globeGenerationRef.current;
      setGlobeEpoch((epoch) => epoch + 1);
      void loadGlobeData(generation, bbox);
    },
    [committedQuery, loadGlobeData],
  );

  // Load the first page for the current filter + committed search query.
  const runQuery = useCallback(async () => {
    const generation = ++queryGenerationRef.current;
    setError(null);
    setNewEventsCount(0);
    setEvents([]);
    setGlobeEvents([]);
    setGlobeTotal(0);
    setClusterFocus(null);
    setGlobeEpoch((epoch) => epoch + 1);
    const token = await getAccessToken();
    if (!token || generation !== queryGenerationRef.current) {
      return;
    }
    setAccessToken(token);
    setLoading(true);
    try {
      if (committedQuery) {
        const results = await searchEvents(token, committedQuery, filters, sources);
        if (generation !== queryGenerationRef.current) {
          return;
        }
        const mapped = results.hits.map(searchHitToEvent);
        setEvents(mapped);
        const searchQuakes = quakesFrom(mapped);
        setGlobeEvents(searchQuakes);
        setGlobeTotal(searchQuakes.length);
        setTotalEvents(results.estimated_total);
        setHasMore(false);
        setSelectedId(mapped[0]?.id ?? null);
      } else {
        const response = await listEvents(token, {
          filters,
          availableSources: sources,
          limit: PAGE_SIZE,
          offset: 0,
        });
        if (generation !== queryGenerationRef.current) {
          return;
        }
        setEvents(response.items);
        setTotalEvents(response.total);
        setHasMore(response.offset + response.items.length < response.total);
        setSelectedId((current) => current ?? response.items[0]?.id ?? null);
        // New globe generation; keep the current viewport scope across filter changes.
        const globeGeneration = ++globeGenerationRef.current;
        void loadGlobeData(globeGeneration, globeBboxRef.current);
      }
    } catch (err) {
      if (generation !== queryGenerationRef.current) {
        return;
      }
      setError(err instanceof ApiError ? err.message : "Failed to load events");
    } finally {
      if (generation === queryGenerationRef.current) {
        setLoading(false);
      }
    }
  }, [getAccessToken, committedQuery, filters, sources, searchHitToEvent, loadGlobeData]);

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- data fetch on filter/query change
    void runQuery();
  }, [runQuery]);

  // Load distinct source options for the filter once authenticated.
  useEffect(() => {
    if (!enabled) {
      return;
    }
    void (async () => {
      const token = await getAccessToken();
      if (!token) {
        return;
      }
      try {
        setSources(await listEventSources(token));
      } catch {
        // Non-fatal: the source filter simply shows nothing.
      }
    })();
  }, [enabled, getAccessToken]);

  const loadMore = useCallback(async () => {
    if (loadingMore || !hasMore || committedQuery) {
      return;
    }
    const generation = queryGenerationRef.current;
    const token = await getAccessToken();
    if (!token || generation !== queryGenerationRef.current) {
      return;
    }
    setLoadingMore(true);
    try {
      const response = await listEvents(token, {
        filters,
        availableSources: sources,
        limit: PAGE_SIZE,
        offset: events.length,
      });
      if (generation !== queryGenerationRef.current) {
        return;
      }
      setEvents((current) => {
        const seen = new Set(current.map((event) => event.id));
        return [...current, ...response.items.filter((event) => !seen.has(event.id))];
      });
      setTotalEvents(response.total);
      setHasMore(response.offset + response.items.length < response.total);
    } catch (err) {
      if (generation !== queryGenerationRef.current) {
        return;
      }
      setError(err instanceof ApiError ? err.message : "Failed to load more events");
    } finally {
      if (generation === queryGenerationRef.current) {
        setLoadingMore(false);
      }
    }
  }, [getAccessToken, filters, sources, events.length, hasMore, loadingMore, committedQuery]);

  // Live stream: list follows sort rules (§11); globe always upserts matching quakes.
  const onStreamEvent = useCallback(
    (event: Event) => {
      if (committedQuery || !eventMatchesFilters(event, filters)) {
        return;
      }
      if (isQuake(event)) {
        setGlobeEvents((current) => mergeGlobeEvent(current, event));
      }
      if (filters.sort === "recent") {
        setEvents((current) => mergeEvent(current, event));
      } else {
        setNewEventsCount((count) => count + 1);
      }
    },
    [committedQuery, filters],
  );

  const { connected } = useEventStream({
    enabled: Boolean(accessToken),
    accessToken,
    onEvent: onStreamEvent,
  });

  function onSearch(event: FormEvent) {
    event.preventDefault();
    setCommittedQuery(searchQuery.trim());
  }

  // One-way globe-layer sync: categories excluded by the filter hide their layers.
  const allCategories = filters.categories.length === 0;
  const quakeAllowed = allCategories || filters.categories.includes("earthquake");
  const weatherAllowed = allCategories || filters.categories.includes("weather");
  const effectiveLayers: GlobeLayers = {
    quakeDots: layers.quakeDots && quakeAllowed,
    quakeHeat: layers.quakeHeat && quakeAllowed,
    weather: layers.weather && weatherAllowed,
  };
  const lockedLayerKeys = {
    quakeDots: !quakeAllowed,
    quakeHeat: !quakeAllowed,
    weather: !weatherAllowed,
  };

  const ingestionPanelRight = filtersOpen ? RIGHT_OF_FILTERS_PANEL : "right-4";
  const ingestionPanelBottom = filtersOpen ? "bottom-4" : "bottom-[4.5rem]";
  const fabClusterRight = filtersOpen ? RIGHT_OF_FILTERS_PANEL : "right-4";

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
          <GlobeViewport
            events={globeEvents}
            selectedId={selectedId}
            onSelect={setSelectedId}
            onSelectCluster={handleSelectCluster}
            onClearSelection={() => {
              setSelectedId(null);
              setClusterFocus(null);
            }}
            layers={effectiveLayers}
            globeTotal={globeTotal}
            globeLoading={globeLoading}
            globeLoadingMore={globeLoadingMore}
            globeEpoch={globeEpoch}
            onViewportChange={handleViewportChange}
          />
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
            <div className="flex flex-col gap-0.5">
              <span className={CARD_LABEL_CLASS}>Events</span>
              {!loading && !committedQuery && totalEvents > 0 ? (
                <span className="text-[10px] tabular-nums text-foreground/45">
                  Showing {events.length.toLocaleString()} of {totalEvents.toLocaleString()}
                </span>
              ) : null}
            </div>
            <button
              type="button"
              aria-label="Collapse events panel"
              className={CARD_TOGGLE_CLASS}
              onClick={() => setLeftOpen(false)}
            >
              <ChevronLeft size={16} />
            </button>
          </div>
          {newEventsCount > 0 ? (
            <button
              type="button"
              className="mx-3 mt-2 rounded-full border border-primary/40 bg-primary/15 px-3 py-1 text-xs font-medium text-primary transition-colors hover:bg-primary/25"
              onClick={() => void runQuery()}
            >
              {newEventsCount} new event{newEventsCount === 1 ? "" : "s"} · refresh
            </button>
          ) : null}
          <div className="min-h-0 flex-1 overflow-y-auto">
            {clusterFocus ? (
              <>
                <div className="flex items-center justify-between gap-2 border-b border-white/10 bg-white/5 px-3 py-2">
                  <span className="text-[11px] font-medium text-foreground/70">
                    Cluster · {clusterFocus.count.toLocaleString()} quake
                    {clusterFocus.count === 1 ? "" : "s"}
                    {clusterFocus.count > clusterFocus.events.length
                      ? ` · top ${clusterFocus.events.length.toLocaleString()}`
                      : ""}
                  </span>
                  <button
                    type="button"
                    className="shrink-0 rounded-full px-2 py-0.5 text-[11px] font-medium text-primary transition-colors hover:bg-white/10"
                    onClick={() => setClusterFocus(null)}
                  >
                    Clear
                  </button>
                </div>
                <EventList
                  events={clusterFocus.events}
                  selectedId={selectedId}
                  onSelect={setSelectedId}
                />
              </>
            ) : loading ? (
              <p className="p-4 text-sm text-foreground/50">Loading…</p>
            ) : (
              <>
                <EventList events={events} selectedId={selectedId} onSelect={setSelectedId} />
                {hasMore ? (
                  <div className="p-3">
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      className="w-full rounded-lg hover:bg-white/10"
                      disabled={loadingMore}
                      onClick={() => void loadMore()}
                    >
                      {loadingMore
                        ? "Loading…"
                        : `Load more (${(totalEvents - events.length).toLocaleString()} remaining)`}
                    </Button>
                  </div>
                ) : null}
              </>
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

      {/* Visualization layers rail: hugs the left edge, but slides to the right
          of the events panel while it is open so the two never overlap. */}
      <LayersRail
        layers={layers}
        onChange={setLayers}
        side="left"
        lockedKeys={lockedLayerKeys}
        className={`top-1/2 -translate-y-1/2 transition-[left] duration-300 ease-out ${
          leftOpen ? RIGHT_OF_EVENTS_PANEL : "left-4"
        }`}
      />

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

      {/* Floating ingestion/backfill status panel (bottom-right, collapsible). */}
      {ingestionOpen ? (
        <aside
          className={`absolute z-30 w-[min(19rem,calc(100vw-2rem))] ${ingestionPanelBottom} ${ingestionPanelRight} ${FLOATING_CARD_CLASS}`}
        >
          <div className="flex items-center justify-between border-b border-white/10 px-4 py-3">
            <span className={CARD_LABEL_CLASS}>Data ingestion</span>
            <button
              type="button"
              aria-label="Close ingestion panel"
              className={CARD_TOGGLE_CLASS}
              onClick={() => setIngestionOpen(false)}
            >
              <X size={16} />
            </button>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">
            <IngestionStatus
              connectors={connectors}
              loading={connectorsLoading}
              error={connectorsError}
              updatedAt={connectorsUpdatedAt}
              deltas={connectorDeltas}
            />
          </div>
        </aside>
      ) : null}

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
            <FilterPanel
              filters={filters}
              onChange={setFilters}
              sources={sources}
              presets={presets}
              canManagePresets={canManagePresets}
              onSavePreset={async (name) => {
                await savePreset(name, filters);
              }}
              onUpdatePreset={async (id, name) => {
                await updatePreset(id, name, filters);
              }}
              onDeletePreset={async (id) => {
                await removePreset(id);
              }}
            />
          </div>
        </aside>
      ) : null}

      {/* Bottom-right FAB cluster: ingestion (left) + filters (right). */}
      {!ingestionOpen || !filtersOpen ? (
        <div className={`absolute bottom-4 z-20 flex items-center gap-3 ${fabClusterRight}`}>
          {!ingestionOpen ? (
            <button
              type="button"
              aria-label="Open ingestion panel"
              className={FAB_CLASS}
              onClick={() => setIngestionOpen(true)}
            >
              <Activity size={18} />
              {connectors.some((c) => c.enabled && !c.backfill.complete && c.backfill.initialized) ? (
                <span className="absolute right-2 top-2 size-2 animate-pulse rounded-full bg-primary" aria-hidden />
              ) : null}
            </button>
          ) : null}
          {!filtersOpen ? (
            <button
              type="button"
              aria-label="Open filters panel"
              className={FAB_CLASS}
              onClick={() => setFiltersOpen(true)}
            >
              <SlidersHorizontal size={18} />
              {filtersAreActive(filters) ? (
                <span className="absolute right-2 top-2 size-2 rounded-full bg-primary" aria-hidden />
              ) : null}
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
