import { useEffect, useRef, useState } from "react";
import type { Event } from "@/types/event";
import { streamUrl, type StreamEnvelope } from "@/lib/events-api";

interface UseEventStreamOptions {
  enabled: boolean;
  accessToken: string | null;
  onEvent: (event: Event) => void;
}

/** Subscribe to live event upserts over WebSocket. */
export function useEventStream({ enabled, accessToken, onEvent }: UseEventStreamOptions): {
  connected: boolean;
} {
  const [socketConnected, setSocketConnected] = useState(false);
  const onEventRef = useRef(onEvent);

  useEffect(() => {
    onEventRef.current = onEvent;
  }, [onEvent]);

  useEffect(() => {
    if (!enabled || !accessToken) {
      return undefined;
    }

    let closed = false;
    let socket: WebSocket | null = null;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;

    const connect = () => {
      if (closed) {
        return;
      }
      socket = new WebSocket(streamUrl(accessToken));

      socket.addEventListener("open", () => {
        if (!closed) {
          setSocketConnected(true);
        }
      });

      socket.addEventListener("message", (message) => {
        try {
          const envelope = JSON.parse(String(message.data)) as StreamEnvelope;
          if (envelope.kind === "event.upsert" && envelope.event) {
            onEventRef.current(envelope.event);
          }
        } catch {
          // Ignore malformed frames.
        }
      });

      socket.addEventListener("close", () => {
        setSocketConnected(false);
        if (!closed) {
          retryTimer = setTimeout(connect, 3_000);
        }
      });

      socket.addEventListener("error", () => {
        socket?.close();
      });
    };

    connect();

    return () => {
      closed = true;
      if (retryTimer) {
        clearTimeout(retryTimer);
      }
      socket?.close();
      setSocketConnected(false);
    };
  }, [enabled, accessToken]);

  return { connected: enabled && accessToken !== null && socketConnected };
}
