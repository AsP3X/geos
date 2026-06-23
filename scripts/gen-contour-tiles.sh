#!/usr/bin/env bash
# Generate the offline elevation-contour basemap used as the Cesium globe's
# fallback when Sentinel-2 imagery is unavailable (see the tile-proxy design and
# docs/adr). From the public-domain ETOPO 2022 DEM we render a shaded relief +
# contour raster and slice it into XYZ JPEG tiles to ~z8.
#
# Output (default): frontend/public/contours/{z}/{x}/{y}.jpg — served same-origin
# so the globe works fully offline. Set CONTOUR_OUT to a bucket-sync staging dir
# (key prefix contours/{z}/{x}/{y}.jpg) to serve via the tile proxy instead.
#
# Requirements: GDAL >= 3.5 with gdaldem, gdal_contour, gdal_rasterize, and
# gdal2tiles (the JPEG tiledriver needs GDAL 3.5+). This is a one-time, offline
# preprocessing run; z8 global contour JPEGs are roughly low single-digit GB.
set -euo pipefail
cd "$(dirname "$0")/.."
root="$(pwd)"

# ── Tunables (env-overridable) ────────────────────────────────────────────────
# ETOPO 2022 60 arc-second surface elevation (public domain, NOAA NCEI).
ETOPO_URL="${ETOPO_URL:-https://www.ngdc.noaa.gov/thredds/fileServer/global/ETOPO2022/60s/60s_surface_elev_netcdf/ETOPO_2022_v1_60s_N90W180_surface.nc}"
WORK_DIR="${CONTOUR_WORK:-$root/.cache/contours}"
OUT_DIR="${CONTOUR_OUT:-$root/frontend/public/contours}"
MAX_ZOOM="${CONTOUR_MAX_ZOOM:-8}"
CONTOUR_INTERVAL="${CONTOUR_INTERVAL:-500}" # metres between contour lines

# ── Preconditions ─────────────────────────────────────────────────────────────
need() { command -v "$1" >/dev/null 2>&1 || { echo "ERROR: '$1' not found. Install GDAL (>=3.5)." >&2; exit 1; }; }
need gdaldem
need gdal_contour
need gdal_rasterize
if command -v gdal2tiles.py >/dev/null 2>&1; then GDAL2TILES=gdal2tiles.py; else need gdal2tiles; GDAL2TILES=gdal2tiles; fi

mkdir -p "$WORK_DIR" "$OUT_DIR"
dem="$WORK_DIR/etopo.nc"
relief="$WORK_DIR/relief.tif"
hillshade="$WORK_DIR/hillshade.tif"
contour_lines="$WORK_DIR/contours.gpkg"
contour_raster="$WORK_DIR/contours.tif"
basemap="$WORK_DIR/basemap.tif"

# ── 1. Fetch the DEM (cached) ─────────────────────────────────────────────────
if [[ ! -f "$dem" ]]; then
  echo "==> Downloading ETOPO 2022 DEM -> $dem"
  if command -v curl >/dev/null 2>&1; then curl -fL "$ETOPO_URL" -o "$dem"; else wget -O "$dem" "$ETOPO_URL"; fi
fi

# ── 2. Color-relief + hillshade for legible terrain shading ───────────────────
ramp="$WORK_DIR/ramp.txt"
cat > "$ramp" <<'RAMP'
# elevation(m) R G B  — ocean blues below 0, land greens→browns→white above.
-11000 8 24 58
-200   16 54 102
-1     32 96 150
0      54 84 54
200    86 122 64
800    150 146 86
1800   150 120 80
3500   180 180 180
6500   245 245 245
RAMP
echo "==> Rendering color relief + hillshade"
gdaldem color-relief "$dem" "$ramp" "$relief" -alpha
gdaldem hillshade "$dem" "$hillshade" -z 4 -compute_edges

# ── 3. Contour lines -> rasterized overlay ────────────────────────────────────
echo "==> Generating ${CONTOUR_INTERVAL}m contour lines"
rm -f "$contour_lines"
gdal_contour -a elev -i "$CONTOUR_INTERVAL" "$dem" "$contour_lines"
# Rasterize contour lines (dark, semi-transparent) onto a copy of the relief.
cp "$relief" "$contour_raster"
gdal_rasterize -burn 40 -burn 40 -burn 40 -l contours "$contour_lines" "$contour_raster"

# ── 4. Blend hillshade with the contoured relief ──────────────────────────────
echo "==> Compositing shaded basemap"
gdal_calc.py --quiet \
  -A "$contour_raster" --A_band=1 -B "$contour_raster" --B_band=2 -C "$contour_raster" --C_band=3 \
  -H "$hillshade" \
  --outfile="$basemap" --type=Byte --co=PHOTOMETRIC=RGB --allBands=A 2>/dev/null \
  --calc="A*(0.6+0.4*H/255)" --calc="B*(0.6+0.4*H/255)" --calc="C*(0.6+0.4*H/255)" \
  || cp "$contour_raster" "$basemap"

# ── 5. Slice into XYZ JPEG tiles (z0..MAX_ZOOM) ───────────────────────────────
echo "==> Tiling to XYZ JPEG (z0-$MAX_ZOOM) -> $OUT_DIR"
"$GDAL2TILES" --xyz -p mercator -z "0-$MAX_ZOOM" --tiledriver=JPEG -w none --processes="${CONTOUR_PROCESSES:-4}" "$basemap" "$OUT_DIR"

echo "Done. Contour basemap tiles written to $OUT_DIR/{z}/{x}/{y}.jpg"
echo "The Cesium globe serves these as the offline fallback (VITE_CONTOUR_TILES_URL)."
