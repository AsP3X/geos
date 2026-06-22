import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError } from "@/lib/api-client";
import { listConnectors, type ConnectorStatus } from "@/lib/connectors-api";

interface UseConnectorStatusOptions {
  enabled: boolean;
  getAccessToken: () => Promise<string | null>;
  /** Poll interval in milliseconds (default 250ms). */
  intervalMs?: number;
}

interface ConnectorDelta {
  /** Events ingested since the previous successful poll. */
  eventsDelta: number;
}

interface UseConnectorStatusResult {
  connectors: ConnectorStatus[];
  loading: boolean;
  error: string | null;
  /** When the status was last fetched successfully. */
  updatedAt: Date | null;
  /** Per-source deltas since the previous poll. */
  deltas: Record<string, ConnectorDelta>;
}

/** Poll the connector ingestion + backfill status endpoint while enabled. */
export function useConnectorStatus({
  enabled,
  getAccessToken,
  intervalMs = 250,
}: UseConnectorStatusOptions): UseConnectorStatusResult {
  const [connectors, setConnectors] = useState<ConnectorStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);
  const [deltas, setDeltas] = useState<Record<string, ConnectorDelta>>({});
  const getTokenRef = useRef(getAccessToken);
  const prevEventsRef = useRef<Record<string, number>>({});

  useEffect(() => {
    getTokenRef.current = getAccessToken;
  }, [getAccessToken]);

  const refresh = useCallback(async () => {
    const token = await getTokenRef.current();
    if (!token) {
      setError("Session expired — sign in again to refresh ingestion status");
      setLoading(false);
      return;
    }
    try {
      const response = await listConnectors(token);
      const nextDeltas: Record<string, ConnectorDelta> = {};
      for (const connector of response.connectors) {
        const prev = prevEventsRef.current[connector.source_key];
        const current = connector.backfill.events_ingested;
        nextDeltas[connector.source_key] = {
          eventsDelta: prev === undefined ? 0 : Math.max(0, current - prev),
        };
        prevEventsRef.current[connector.source_key] = current;
      }
      setConnectors(response.connectors);
      setDeltas(nextDeltas);
      setUpdatedAt(new Date());
      setError(null);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Failed to load ingestion status");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!enabled) {
      return undefined;
    }
    let cancelled = false;
    const tick = () => {
      if (!cancelled) {
        void refresh();
      }
    };
    tick();
    const timer = setInterval(tick, intervalMs);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [enabled, intervalMs, refresh]);

  return { connectors, loading, error, updatedAt, deltas };
}
