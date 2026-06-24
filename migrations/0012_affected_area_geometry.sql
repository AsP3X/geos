-- 0012 — Widen events.affected_area to generic geometry for NWS MultiPolygon zones.
--
-- NWS weather alerts are often MultiPolygon (multiple forecast zones). The
-- canonical Event.affected_area field stores alert boundaries as GeoJSON-aligned
-- PostGIS geometry (geospatial-postgis.mdc).

ALTER TABLE events
    ALTER COLUMN affected_area TYPE geometry(Geometry, 4326)
    USING affected_area::geometry(Geometry, 4326);
