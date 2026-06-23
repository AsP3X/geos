import { useCallback, useEffect, useState } from "react";
import type { EventFilters } from "@/components/filters/filters";
import {
  createSavedFilter,
  deleteSavedFilter,
  listSavedFilters,
  type SavedFilter,
  updateSavedFilter,
} from "@/lib/saved-filters-api";

interface UseSavedFiltersOptions {
  enabled: boolean;
  getAccessToken: () => Promise<string | null>;
}

/** Load and mutate the caller's saved filter presets (RBAC-gated server-side). */
export function useSavedFilters({ enabled, getAccessToken }: UseSavedFiltersOptions): {
  presets: SavedFilter[];
  loading: boolean;
  reload: () => Promise<void>;
  save: (name: string, filters: EventFilters) => Promise<SavedFilter>;
  update: (id: string, name: string, filters: EventFilters) => Promise<SavedFilter>;
  remove: (id: string) => Promise<void>;
} {
  const [presets, setPresets] = useState<SavedFilter[]>([]);
  const [loading, setLoading] = useState(false);

  const reload = useCallback(async () => {
    const token = await getAccessToken();
    if (!token) {
      return;
    }
    setLoading(true);
    try {
      setPresets(await listSavedFilters(token));
    } finally {
      setLoading(false);
    }
  }, [getAccessToken]);

  useEffect(() => {
    if (enabled) {
      // eslint-disable-next-line react-hooks/set-state-in-effect -- async data fetch
      void reload();
    }
  }, [enabled, reload]);

  const save = useCallback(
    async (name: string, filters: EventFilters) => {
      const token = await getAccessToken();
      if (!token) {
        throw new Error("Not authenticated");
      }
      const saved = await createSavedFilter(token, name, filters);
      await reload();
      return saved;
    },
    [getAccessToken, reload],
  );

  const update = useCallback(
    async (id: string, name: string, filters: EventFilters) => {
      const token = await getAccessToken();
      if (!token) {
        throw new Error("Not authenticated");
      }
      const saved = await updateSavedFilter(token, id, name, filters);
      await reload();
      return saved;
    },
    [getAccessToken, reload],
  );

  const remove = useCallback(
    async (id: string) => {
      const token = await getAccessToken();
      if (!token) {
        throw new Error("Not authenticated");
      }
      await deleteSavedFilter(token, id);
      await reload();
    },
    [getAccessToken, reload],
  );

  return { presets, loading, reload, save, update, remove };
}
