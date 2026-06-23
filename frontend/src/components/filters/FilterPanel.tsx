import { useState } from "react";
import { Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  CATEGORIES,
  DEFAULT_FILTERS,
  type EventFilters,
  filtersAreActive,
  SEVERITIES,
  SORT_OPTIONS,
  TIME_RANGES,
} from "@/components/filters/filters";
import type { SavedFilter } from "@/lib/saved-filters-api";
import { cn } from "@/lib/utils";

const FIELD_LABEL_CLASS =
  "text-[10px] font-medium uppercase tracking-[0.14em] text-foreground/45";

const SELECT_CLASS =
  "h-8 w-full rounded-lg border border-white/10 bg-white/5 px-2 text-sm text-foreground/90 capitalize outline-none focus-visible:ring-2 focus-visible:ring-primary/40";

const CHIP_BASE =
  "rounded-full border px-2.5 py-1 text-xs capitalize transition-colors";

interface FilterPanelProps {
  filters: EventFilters;
  onChange: (filters: EventFilters) => void;
  sources: string[];
  presets: SavedFilter[];
  canManagePresets: boolean;
  onSavePreset: (name: string) => Promise<void>;
  onUpdatePreset: (id: string, name: string) => Promise<void>;
  onDeletePreset: (id: string) => Promise<void>;
}

function toggleValue<T>(list: T[], value: T): T[] {
  return list.includes(value) ? list.filter((item) => item !== value) : [...list, value];
}

/** Convert an ISO string to a `datetime-local` input value (local time). */
function isoToLocalInput(iso: string | null): string {
  if (!iso) {
    return "";
  }
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return "";
  }
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}`;
}

function localInputToIso(value: string): string | null {
  if (!value) {
    return null;
  }
  const ms = Date.parse(value);
  return Number.isNaN(ms) ? null : new Date(ms).toISOString();
}

/** Like `localInputToIso`, but midnight on the "to" field means end of that local day. */
function localInputToIsoEndOfDay(value: string): string | null {
  if (!value) {
    return null;
  }
  const ms = Date.parse(value);
  if (Number.isNaN(ms)) {
    return null;
  }
  const date = new Date(ms);
  if (date.getHours() === 0 && date.getMinutes() === 0) {
    date.setHours(23, 59, 59, 999);
  }
  return date.toISOString();
}

/** Multi-dimensional event filter with saved presets. */
export function FilterPanel({
  filters,
  onChange,
  sources,
  presets,
  canManagePresets,
  onSavePreset,
  onUpdatePreset,
  onDeletePreset,
}: FilterPanelProps) {
  const [selectedPresetId, setSelectedPresetId] = useState<string>("");
  const [presetName, setPresetName] = useState("");
  const [busy, setBusy] = useState(false);

  // Any manual edit clears the selected preset (filters no longer match it).
  function update(next: EventFilters) {
    setSelectedPresetId("");
    onChange(next);
  }

  function applyPreset(id: string) {
    const preset = presets.find((p) => p.id === id);
    if (!preset) {
      return;
    }
    setSelectedPresetId(id);
    setPresetName(preset.name);
    onChange(preset.filters);
  }

  async function runPresetAction(action: () => Promise<void>) {
    setBusy(true);
    try {
      await action();
    } finally {
      setBusy(false);
    }
  }

  const selectedPreset = presets.find((p) => p.id === selectedPresetId);

  return (
    <div className="flex flex-col gap-4 p-4">
      {/* Presets */}
      <div className="flex flex-col gap-1.5">
        <label className={FIELD_LABEL_CLASS} htmlFor="filter-preset">
          Preset
        </label>
        <select
          id="filter-preset"
          className={cn(SELECT_CLASS, "normal-case")}
          value={selectedPresetId}
          onChange={(event) => {
            const id = event.target.value;
            if (id) {
              applyPreset(id);
            } else {
              setSelectedPresetId("");
            }
          }}
        >
          <option value="">{presets.length > 0 ? "Select a preset…" : "No presets"}</option>
          {presets.map((preset) => (
            <option key={preset.id} value={preset.id}>
              {preset.name}
            </option>
          ))}
        </select>

        {canManagePresets ? (
          <div className="flex flex-col gap-1.5">
            <div className="flex items-center gap-1.5">
              <Input
                className="h-8 flex-1 rounded-lg border-white/10 bg-white/5 text-sm"
                placeholder="Preset name"
                value={presetName}
                onChange={(event) => setPresetName(event.target.value)}
              />
              <Button
                type="button"
                size="sm"
                variant="ghost"
                className="rounded-lg hover:bg-white/10"
                disabled={busy || presetName.trim().length === 0}
                onClick={() =>
                  void runPresetAction(async () => {
                    await onSavePreset(presetName.trim());
                    setPresetName("");
                  })
                }
              >
                Save
              </Button>
            </div>
            {selectedPreset ? (
              <div className="flex items-center gap-1.5">
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  className="flex-1 rounded-lg hover:bg-white/10"
                  disabled={busy || presetName.trim().length === 0}
                  onClick={() =>
                    void runPresetAction(() =>
                      onUpdatePreset(selectedPreset.id, presetName.trim()),
                    )
                  }
                >
                  Update “{selectedPreset.name}”
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  aria-label="Delete preset"
                  className="rounded-lg text-destructive hover:bg-destructive/10"
                  disabled={busy}
                  onClick={() =>
                    void runPresetAction(async () => {
                      await onDeletePreset(selectedPreset.id);
                      setSelectedPresetId("");
                      setPresetName("");
                    })
                  }
                >
                  <Trash2 size={14} />
                </Button>
              </div>
            ) : null}
          </div>
        ) : null}
      </div>

      <div className="h-px bg-white/10" />

      {/* Time range */}
      <div className="flex flex-col gap-1.5">
        <label className={FIELD_LABEL_CLASS}>Time range</label>
        <div className="flex flex-wrap gap-1.5">
          {TIME_RANGES.map((range) => (
            <button
              key={range.value}
              type="button"
              onClick={() => update({ ...filters, timeRange: range.value })}
              className={cn(
                CHIP_BASE,
                "normal-case",
                filters.timeRange === range.value
                  ? "border-primary/60 bg-primary/15 text-primary"
                  : "border-white/10 bg-white/5 text-foreground/70 hover:text-foreground",
              )}
            >
              {range.label}
            </button>
          ))}
        </div>
        {filters.timeRange === "custom" ? (
          <div className="mt-1 flex flex-col gap-1.5">
            <input
              type="datetime-local"
              className={SELECT_CLASS}
              value={isoToLocalInput(filters.from)}
              onChange={(event) =>
                update({ ...filters, from: localInputToIso(event.target.value) })
              }
            />
            <input
              type="datetime-local"
              className={SELECT_CLASS}
              value={isoToLocalInput(filters.to)}
              onChange={(event) =>
                update({ ...filters, to: localInputToIsoEndOfDay(event.target.value) })
              }
            />
          </div>
        ) : null}
      </div>

      {/* Categories */}
      <div className="flex flex-col gap-1.5">
        <label className={FIELD_LABEL_CLASS}>Categories</label>
        <div className="flex flex-wrap gap-1.5">
          {CATEGORIES.map((category) => {
            const active = filters.categories.includes(category);
            return (
              <button
                key={category}
                type="button"
                aria-pressed={active}
                onClick={() =>
                  update({ ...filters, categories: toggleValue(filters.categories, category) })
                }
                className={cn(
                  CHIP_BASE,
                  active
                    ? "border-primary/60 bg-primary/15 text-primary"
                    : "border-white/10 bg-white/5 text-foreground/70 hover:text-foreground",
                )}
              >
                {category}
              </button>
            );
          })}
        </div>
      </div>

      {/* Severities */}
      <div className="flex flex-col gap-1.5">
        <label className={FIELD_LABEL_CLASS}>Severity</label>
        <div className="flex flex-wrap gap-1.5">
          {SEVERITIES.map((severity) => {
            const active = filters.severities.includes(severity);
            return (
              <button
                key={severity}
                type="button"
                aria-pressed={active}
                onClick={() =>
                  update({ ...filters, severities: toggleValue(filters.severities, severity) })
                }
                className={cn(
                  CHIP_BASE,
                  active
                    ? "border-primary/60 bg-primary/15 text-primary"
                    : "border-white/10 bg-white/5 text-foreground/70 hover:text-foreground",
                )}
              >
                {severity}
              </button>
            );
          })}
        </div>
      </div>

      {/* Sources */}
      {sources.length > 0 ? (
        <div className="flex flex-col gap-1.5">
          <label className={FIELD_LABEL_CLASS}>Sources</label>
          <div className="flex flex-wrap gap-1.5">
            {sources.map((source) => {
              const active = filters.sources.includes(source);
              return (
                <button
                  key={source}
                  type="button"
                  aria-pressed={active}
                  onClick={() =>
                    update({ ...filters, sources: toggleValue(filters.sources, source) })
                  }
                  className={cn(
                    CHIP_BASE,
                    "lowercase",
                    active
                      ? "border-primary/60 bg-primary/15 text-primary"
                      : "border-white/10 bg-white/5 text-foreground/70 hover:text-foreground",
                  )}
                >
                  {source}
                </button>
              );
            })}
          </div>
        </div>
      ) : null}

      {/* Impact range */}
      <div className="flex flex-col gap-1.5">
        <div className="flex items-center justify-between">
          <label className={FIELD_LABEL_CLASS}>Impact</label>
          <span className="text-xs font-semibold tabular-nums text-primary">
            {filters.impactMin}–{filters.impactMax}
          </span>
        </div>
        <input
          type="range"
          aria-label="Minimum impact"
          min={0}
          max={100}
          step={5}
          value={filters.impactMin}
          onChange={(event) =>
            update({
              ...filters,
              impactMin: Math.min(Number(event.target.value), filters.impactMax),
            })
          }
          className="h-1.5 w-full cursor-pointer appearance-none rounded-full bg-white/10 accent-primary"
        />
        <input
          type="range"
          aria-label="Maximum impact"
          min={0}
          max={100}
          step={5}
          value={filters.impactMax}
          onChange={(event) =>
            update({
              ...filters,
              impactMax: Math.max(Number(event.target.value), filters.impactMin),
            })
          }
          className="h-1.5 w-full cursor-pointer appearance-none rounded-full bg-white/10 accent-primary"
        />
      </div>

      {/* Magnitude range */}
      <div className="flex flex-col gap-1.5">
        <label className={FIELD_LABEL_CLASS}>Magnitude</label>
        <div className="flex items-center gap-1.5">
          <Input
            type="number"
            inputMode="decimal"
            step={0.1}
            placeholder="Any"
            className="h-8 rounded-lg border-white/10 bg-white/5 text-sm"
            value={filters.magnitudeMin ?? ""}
            onChange={(event) =>
              update({
                ...filters,
                magnitudeMin: event.target.value === "" ? null : Number(event.target.value),
              })
            }
          />
          <span className="text-foreground/40">–</span>
          <Input
            type="number"
            inputMode="decimal"
            step={0.1}
            placeholder="Any"
            className="h-8 rounded-lg border-white/10 bg-white/5 text-sm"
            value={filters.magnitudeMax ?? ""}
            onChange={(event) =>
              update({
                ...filters,
                magnitudeMax: event.target.value === "" ? null : Number(event.target.value),
              })
            }
          />
        </div>
        <p className="text-[10px] text-foreground/40">
          Events without a magnitude are excluded when set.
        </p>
      </div>

      {/* Sort */}
      <div className="flex flex-col gap-1.5">
        <label className={FIELD_LABEL_CLASS} htmlFor="filter-sort">
          Sort
        </label>
        <select
          id="filter-sort"
          className={cn(SELECT_CLASS, "normal-case")}
          value={filters.sort}
          onChange={(event) =>
            update({ ...filters, sort: event.target.value as EventFilters["sort"] })
          }
        >
          {SORT_OPTIONS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </div>

      <Button
        type="button"
        size="sm"
        variant="ghost"
        className="rounded-lg hover:bg-white/10"
        onClick={() => update({ ...DEFAULT_FILTERS })}
        disabled={!filtersAreActive(filters)}
      >
        Reset filters
      </Button>
    </div>
  );
}
