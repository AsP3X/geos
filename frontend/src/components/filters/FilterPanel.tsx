import type { Event } from "@/types/event";
import { Button } from "@/components/ui/button";
import {
  CATEGORIES,
  type EventFilters,
  DEFAULT_FILTERS,
  filtersAreActive,
  SEVERITIES,
} from "@/components/filters/filters";

const FIELD_LABEL_CLASS =
  "text-[10px] font-medium uppercase tracking-[0.14em] text-foreground/45";

const SELECT_CLASS =
  "h-8 w-full rounded-lg border border-white/10 bg-white/5 px-2 text-sm text-foreground/90 capitalize outline-none focus-visible:ring-2 focus-visible:ring-primary/40";

interface FilterPanelProps {
  filters: EventFilters;
  onChange: (filters: EventFilters) => void;
}

/** Category / severity / impact filters that drive the event query. */
export function FilterPanel({ filters, onChange }: FilterPanelProps) {
  return (
    <div className="flex flex-col gap-4 p-4">
      <div className="flex flex-col gap-1.5">
        <label className={FIELD_LABEL_CLASS} htmlFor="filter-category">
          Category
        </label>
        <select
          id="filter-category"
          className={SELECT_CLASS}
          value={filters.category ?? ""}
          onChange={(event) =>
            onChange({
              ...filters,
              category: (event.target.value || undefined) as Event["category"] | undefined,
            })
          }
        >
          <option value="">All categories</option>
          {CATEGORIES.map((category) => (
            <option key={category} value={category}>
              {category}
            </option>
          ))}
        </select>
      </div>

      <div className="flex flex-col gap-1.5">
        <label className={FIELD_LABEL_CLASS} htmlFor="filter-severity">
          Severity
        </label>
        <select
          id="filter-severity"
          className={SELECT_CLASS}
          value={filters.severity ?? ""}
          onChange={(event) =>
            onChange({
              ...filters,
              severity: (event.target.value || undefined) as Event["severity"] | undefined,
            })
          }
        >
          <option value="">Any severity</option>
          {SEVERITIES.map((severity) => (
            <option key={severity} value={severity}>
              {severity}
            </option>
          ))}
        </select>
      </div>

      <div className="flex flex-col gap-1.5">
        <div className="flex items-center justify-between">
          <label className={FIELD_LABEL_CLASS} htmlFor="filter-impact">
            Min impact
          </label>
          <span className="text-xs font-semibold tabular-nums text-primary">
            {filters.minImpact}
          </span>
        </div>
        <input
          id="filter-impact"
          type="range"
          min={0}
          max={100}
          step={5}
          value={filters.minImpact}
          onChange={(event) =>
            onChange({ ...filters, minImpact: Number(event.target.value) })
          }
          className="h-1.5 w-full cursor-pointer appearance-none rounded-full bg-white/10 accent-primary"
        />
      </div>

      <Button
        type="button"
        size="sm"
        variant="ghost"
        className="rounded-lg hover:bg-white/10"
        onClick={() => onChange(DEFAULT_FILTERS)}
        disabled={!filtersAreActive(filters)}
      >
        Reset filters
      </Button>
    </div>
  );
}
