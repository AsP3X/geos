import {
  ArcType,
  ColorMaterialProperty,
  ConstantProperty,
  GeoJsonDataSource,
  HeightReference,
  JulianDate,
  PointGraphics,
  type Entity,
  type Viewer,
} from "cesium";
import type { Event } from "@/types/event";
import { severityToCesiumColor } from "@/components/globe/cesium/severity-colors";

function weatherColors(severity: Event["severity"], selected: boolean) {
  const base = severityToCesiumColor(severity);
  return {
    fill: base.withAlpha(selected ? 0.35 : 0.2),
    stroke: base.withAlpha(selected ? 0.95 : 0.72),
    strokeWidth: selected ? 3 : 2,
  };
}

function eventGeometry(event: Event): { type: string; coordinates: unknown } {
  if (event.affected_area && typeof event.affected_area === "object") {
    const area = event.affected_area as { type?: string; coordinates?: unknown };
    if (area.type === "Polygon" || area.type === "MultiPolygon") {
      return { type: area.type, coordinates: area.coordinates };
    }
  }
  return {
    type: "Point",
    coordinates: [event.location.lon, event.location.lat],
  };
}

function toFeature(event: Event): {
  type: "Feature";
  id: string;
  geometry: { type: string; coordinates: unknown };
  properties: { geosEventId: string; severity: Event["severity"] };
} {
  return {
    type: "Feature",
    id: event.id,
    geometry: eventGeometry(event),
    properties: {
      geosEventId: event.id,
      severity: event.severity,
    },
  };
}

function resolveEventId(entity: Entity): string | undefined {
  if (typeof entity.id === "string") {
    return entity.id;
  }
  const fromProps = entity.properties?.geosEventId?.getValue(JulianDate.now());
  return typeof fromProps === "string" ? fromProps : undefined;
}

function styleEntity(entity: Entity, event: Event, selected: boolean) {
  const colors = weatherColors(event.severity, selected);
  if (entity.polygon) {
    // v1 globe is a flat ellipsoid with no terrain provider, so polygons must
    // render directly on the surface (height 0, no ground clamping — clamped
    // polygons need the classification/terrain pipeline and draw nothing here).
    entity.polygon.material = new ColorMaterialProperty(colors.fill);
    entity.polygon.height = new ConstantProperty(0);
    entity.polygon.heightReference = new ConstantProperty(HeightReference.NONE);
    entity.polygon.perPositionHeight = new ConstantProperty(false);
    entity.polygon.arcType = new ConstantProperty(ArcType.GEODESIC);
    entity.polygon.outline = new ConstantProperty(true);
    entity.polygon.outlineColor = new ConstantProperty(colors.stroke);
    entity.polygon.outlineWidth = new ConstantProperty(colors.strokeWidth);
  }

  // GeoJsonDataSource renders Point features as billboards (pin images); swap
  // them for a styled PointGraphics matching the severity palette.
  if (entity.billboard) {
    entity.billboard = undefined;
  }
  if (!entity.polygon) {
    entity.point = new PointGraphics({
      pixelSize: selected ? 16 : 14,
      color: colors.fill,
      outlineColor: colors.stroke,
      outlineWidth: selected ? 3 : 2,
      heightReference: HeightReference.NONE,
    });
  }
  entity.id = event.id;
}

/** Rebuild the weather alert overlay from the current event set. */
export async function rebuildWeatherLayer(
  viewer: Viewer,
  dataSourceRef: { current: GeoJsonDataSource | null },
  events: Event[],
  selectedId: string | null,
  visible: boolean,
): Promise<void> {
  if (dataSourceRef.current) {
    viewer.dataSources.remove(dataSourceRef.current, true);
    dataSourceRef.current = null;
  }
  if (!visible || events.length === 0) {
    return;
  }

  const collection = {
    type: "FeatureCollection" as const,
    features: events.map(toFeature),
  };

  // No clampToGround: this globe has no terrain, so polygons render on the
  // ellipsoid surface as ordinary geometry (styled per-entity below).
  const dataSource = await GeoJsonDataSource.load(collection, {
    clampToGround: false,
  });

  for (const entity of dataSource.entities.values) {
    const eventId = resolveEventId(entity);
    const event = events.find((item) => item.id === eventId);
    if (!event) {
      continue;
    }
    styleEntity(entity, event, event.id === selectedId);
  }

  viewer.dataSources.add(dataSource);
  dataSourceRef.current = dataSource;
}

/** Update selection styling without rebuilding the whole layer. */
export function refreshWeatherSelection(
  dataSource: GeoJsonDataSource | null,
  events: Event[],
  selectedId: string | null,
) {
  if (!dataSource) {
    return;
  }
  for (const entity of dataSource.entities.values) {
    const eventId = resolveEventId(entity);
    const event = events.find((item) => item.id === eventId);
    if (!event) {
      continue;
    }
    styleEntity(entity, event, event.id === selectedId);
  }
}

/** Resolve a picked Cesium object to a weather event id, if any. */
export function weatherEventIdFromPick(
  picked: { id?: unknown } | undefined,
  events: Event[],
): string | undefined {
  if (!picked?.id) {
    return undefined;
  }
  const candidate =
    typeof picked.id === "string"
      ? picked.id
      : typeof picked.id === "object" &&
          picked.id !== null &&
          "id" in picked.id &&
          typeof (picked.id as Entity).id === "string"
        ? ((picked.id as Entity).id as string)
        : undefined;
  if (!candidate) {
    return undefined;
  }
  return events.some((event) => event.id === candidate) ? candidate : undefined;
}
