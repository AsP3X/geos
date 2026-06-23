import { CloudRain, Flame, CircleDot, type LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import type { GlobeLayers } from "@/components/globe/layers";

interface LayersRailProps {
  layers: GlobeLayers;
  onChange: (layers: GlobeLayers) => void;
  /** Positioning classes supplied by the page (edge offset + vertical center). */
  className?: string;
  /** Which screen edge the rail is pinned to (controls expand direction). */
  side?: "left" | "right";
  /**
   * Layer keys locked off because their category is excluded by the active
   * filter (one-way globe-layer sync). Rendered disabled.
   */
  lockedKeys?: Partial<Record<keyof GlobeLayers, boolean>>;
}

interface LayerItem {
  key: keyof GlobeLayers;
  icon: LucideIcon;
  label: string;
  disabled?: boolean;
}

const ITEMS: LayerItem[] = [
  { key: "quakeDots", icon: CircleDot, label: "Quakes · Dots" },
  { key: "quakeHeat", icon: Flame, label: "Quakes · Heat" },
  { key: "weather", icon: CloudRain, label: "Weather", disabled: true },
];

/**
 * Edge-pinned visualization rail. Collapsed it reads as a column of squircle
 * icons tucked against the screen edge; hovering anywhere in the rail expands
 * every control into a clearly labeled button while the icons stay pinned to
 * the edge. Toggles are visual-only (they only show/hide globe layers).
 */
export function LayersRail({
  layers,
  onChange,
  className,
  side = "right",
  lockedKeys,
}: LayersRailProps) {
  const onRight = side === "right";

  function toggle(key: keyof GlobeLayers) {
    onChange({ ...layers, [key]: !layers[key] });
  }

  return (
    <div
      className={cn(
        "group absolute z-20 flex flex-col gap-2",
        onRight ? "items-end" : "items-start",
        className,
      )}
    >
      <span
        className={cn(
          "max-h-0 overflow-hidden text-[10px] font-semibold uppercase tracking-[0.18em] text-foreground/45 opacity-0 transition-all duration-200 ease-out group-hover:max-h-4 group-hover:opacity-100",
          onRight ? "mr-1" : "ml-1",
        )}
        aria-hidden
      >
        Layers
      </span>

      {ITEMS.map((item) => {
        const locked = lockedKeys?.[item.key] ?? false;
        const disabled = item.disabled || locked;
        const active = layers[item.key] && !locked;
        const Icon = item.icon;
        const title = item.disabled
          ? `${item.label} (coming soon)`
          : locked
            ? `${item.label} (category filtered out)`
            : item.label;
        return (
          <button
            key={item.key}
            type="button"
            disabled={disabled}
            aria-pressed={active}
            title={title}
            onClick={disabled ? undefined : () => toggle(item.key)}
            className={cn(
              "glass-panel relative flex h-11 w-11 items-center overflow-hidden rounded-2xl transition-[width,color] duration-200 ease-out",
              "group-hover:w-48",
              onRight ? "flex-row-reverse" : "flex-row",
              active ? "text-primary" : "text-foreground/65",
              disabled ? "cursor-not-allowed opacity-40" : "hover:text-foreground",
            )}
          >
            <span className="grid size-11 shrink-0 place-items-center">
              <Icon size={18} />
            </span>
            <span
              className={cn(
                "whitespace-nowrap text-sm font-medium opacity-0 transition-opacity duration-150 group-hover:opacity-100",
                onRight ? "pl-3 text-right" : "pr-3",
              )}
            >
              {item.label}
              {item.disabled ? (
                <span className="ml-1.5 text-[10px] uppercase tracking-wide text-foreground/40">
                  Soon
                </span>
              ) : null}
            </span>
            {active ? (
              <span
                className={cn(
                  "absolute top-2 size-2 rounded-full bg-primary shadow-[0_0_8px] shadow-primary/60",
                  onRight ? "left-2" : "right-2",
                )}
                aria-hidden
              />
            ) : null}
          </button>
        );
      })}
    </div>
  );
}
