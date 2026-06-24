import { useMemo } from "react";

/**
 * Ambient, non-interactive backdrop for the auth screens. It evokes the live
 * command-center globe without pulling in Cesium (which needs an authenticated
 * tile session): a wireframe earth, an atmospheric limb glow, a faint
 * starfield, and pulsing event markers tinted with the app's severity palette.
 */

const VIEW = 600;
const CENTER = VIEW / 2;
const RADIUS = 250;
// Slight vertical squash so the rings read as a tilted sphere, not flat rings.
const SQUASH = 0.94;

/** Severity palette mirrored from the globe markers (severity-colors.ts). */
const SEVERITY = ["#2dd4bf", "#38bdf8", "#f59e0b", "#f97316", "#ef4444"] as const;

type Marker = {
  cx: number;
  cy: number;
  r: number;
  color: string;
  delay: number;
  dur: number;
};

/** Project a lat/lon (degrees) onto the front face of the sphere. */
function project(latDeg: number, lonDeg: number) {
  const lat = (latDeg * Math.PI) / 180;
  const lon = (lonDeg * Math.PI) / 180;
  const x = Math.cos(lat) * Math.sin(lon);
  const y = Math.sin(lat);
  const z = Math.cos(lat) * Math.cos(lon);
  return {
    x: CENTER + x * RADIUS,
    y: CENTER - y * RADIUS * SQUASH,
    front: z > 0.04,
  };
}

export function AuthBackdrop() {
  const { latitudes, longitudes, markers } = useMemo(() => {
    const lat: { cy: number; rx: number; ry: number }[] = [];
    for (let d = -60; d <= 60; d += 30) {
      const rad = (d * Math.PI) / 180;
      const rx = RADIUS * Math.cos(rad);
      lat.push({
        cy: CENTER - Math.sin(rad) * RADIUS * SQUASH,
        rx,
        ry: rx * 0.26,
      });
    }

    const lon: number[] = [];
    for (let d = 0; d < 180; d += 30) {
      lon.push(RADIUS * Math.cos((d * Math.PI) / 180));
    }

    // Hand-placed coordinates roughly tracing populated coastlines so the
    // markers feel like real activity hotspots rather than random noise.
    const coords: [number, number][] = [
      [40, -74], // NE US
      [34, -118], // W US
      [19, -99], // Mexico City
      [-23, -46], // São Paulo
      [51, 0], // London
      [48, 11], // Munich
      [55, 37], // Moscow
      [30, 31], // Cairo
      [-26, 28], // Johannesburg
      [28, 77], // Delhi
      [35, 139], // Tokyo
      [31, 121], // Shanghai
      [1, 104], // Singapore
      [-33, 151], // Sydney
      [25, 55], // Dubai
      [37, 127], // Seoul
    ];

    const mk: Marker[] = [];
    coords.forEach(([la, lo], i) => {
      const p = project(la, lo);
      if (!p.front) return;
      mk.push({
        cx: p.x,
        cy: p.y,
        r: 2.4 + (i % 3),
        color: SEVERITY[i % SEVERITY.length],
        delay: (i % 7) * 0.45,
        dur: 3.6 + (i % 4) * 0.7,
      });
    });

    return { latitudes: lat, longitudes: lon, markers: mk };
  }, []);

  return (
    <div
      aria-hidden
      className="pointer-events-none fixed inset-0 -z-10 overflow-hidden bg-[#05070d]"
    >
      {/* Deep-space vignette. */}
      <div className="absolute inset-0 bg-[radial-gradient(120%_120%_at_50%_-10%,#0b1220_0%,#06080f_45%,#040509_100%)]" />

      {/* Drifting starfield — two layers at different sizes/speeds for depth. */}
      <div
        data-geos-motion
        className="absolute inset-0 [background-image:radial-gradient(1.4px_1.4px_at_24px_36px,#ffffff,transparent),radial-gradient(1.2px_1.2px_at_140px_90px,#dbeafe,transparent),radial-gradient(1.6px_1.6px_at_240px_180px,#ffffff,transparent),radial-gradient(1.8px_1.8px_at_330px_70px,#fde9c8,transparent),radial-gradient(1.2px_1.2px_at_70px_240px,#ffffff,transparent),radial-gradient(1.3px_1.3px_at_410px_300px,#ffffff,transparent),radial-gradient(1.1px_1.1px_at_470px_140px,#cbd5e1,transparent)] [background-size:520px_360px]"
        style={{ animation: "geos-drift 160s linear infinite" }}
      />
      <div
        data-geos-motion
        className="absolute inset-0 opacity-70 [background-image:radial-gradient(1px_1px_at_60px_20px,#ffffff,transparent),radial-gradient(1px_1px_at_180px_120px,#ffffff,transparent),radial-gradient(1px_1px_at_300px_220px,#bfdbfe,transparent),radial-gradient(1px_1px_at_360px_40px,#ffffff,transparent),radial-gradient(1px_1px_at_120px_300px,#ffffff,transparent)] [background-size:360px_340px]"
        style={{ animation: "geos-drift 90s linear infinite reverse" }}
      />

      {/* Amber aurora glow — the brand accent breathing behind the globe. */}
      <div
        data-geos-motion
        className="absolute -right-[12%] top-[8%] h-[42rem] w-[42rem] rounded-full bg-[radial-gradient(circle,oklch(0.78_0.16_70/0.22)_0%,transparent_62%)] blur-2xl"
        style={{ animation: "geos-aurora 14s ease-in-out infinite" }}
      />
      <div
        data-geos-motion
        className="absolute -bottom-[18%] left-[-10%] h-[38rem] w-[38rem] rounded-full bg-[radial-gradient(circle,oklch(0.6_0.13_250/0.18)_0%,transparent_64%)] blur-2xl"
        style={{ animation: "geos-aurora 18s ease-in-out infinite reverse" }}
      />

      {/* Wireframe globe, anchored toward the right so the form sits in the
          calmer left/center region. Scales with the viewport so it stays a
          prominent presence on large screens. */}
      <div
        className="absolute top-1/2 hidden -translate-y-1/2 md:block"
        style={{
          width: "clamp(44rem, 72vw, 70rem)",
          height: "clamp(44rem, 72vw, 70rem)",
          right: "clamp(-22rem, -14vw, -6rem)",
        }}
      >
        <div
          className="geos-globe-spin h-full w-full"
          data-geos-motion
          style={{ animation: "geos-globe-spin 160s linear infinite" }}
        >
          <svg
            viewBox={`0 0 ${VIEW} ${VIEW}`}
            className="h-full w-full"
            fill="none"
          >
            <defs>
              <radialGradient id="geos-globe-fill" cx="42%" cy="38%" r="68%">
                <stop offset="0%" stopColor="#13283f" />
                <stop offset="55%" stopColor="#0b1a2b" />
                <stop offset="100%" stopColor="#050a12" />
              </radialGradient>
              <radialGradient id="geos-limb" cx="50%" cy="50%" r="50%">
                <stop offset="78%" stopColor="transparent" />
                <stop offset="93%" stopColor="oklch(0.78 0.16 70 / 0.28)" />
                <stop offset="100%" stopColor="transparent" />
              </radialGradient>
            </defs>

            {/* Atmospheric limb. */}
            <circle cx={CENTER} cy={CENTER} r={RADIUS + 28} fill="url(#geos-limb)" />

            {/* Sphere body. */}
            <circle cx={CENTER} cy={CENTER} r={RADIUS} fill="url(#geos-globe-fill)" />
            <circle
              cx={CENTER}
              cy={CENTER}
              r={RADIUS}
              stroke="oklch(0.78 0.16 70 / 0.4)"
              strokeWidth={1}
            />

            {/* Latitude rings. */}
            {latitudes.map((l, i) => (
              <ellipse
                key={`lat-${i}`}
                cx={CENTER}
                cy={l.cy}
                rx={l.rx}
                ry={l.ry}
                stroke="oklch(0.7 0.04 220 / 0.22)"
                strokeWidth={0.8}
              />
            ))}

            {/* Longitude rings. */}
            {longitudes.map((rx, i) => (
              <ellipse
                key={`lon-${i}`}
                cx={CENTER}
                cy={CENTER}
                rx={rx}
                ry={RADIUS}
                stroke="oklch(0.7 0.04 220 / 0.18)"
                strokeWidth={0.8}
              />
            ))}

            {/* Live event markers with a soft ping halo. */}
            {markers.map((m, i) => (
              <g key={`mk-${i}`}>
                <circle
                  cx={m.cx}
                  cy={m.cy}
                  r={m.r}
                  fill={m.color}
                  opacity={0.18}
                  data-geos-motion
                  style={{
                    transformBox: "fill-box",
                    transformOrigin: "center",
                    animation: `geos-ping ${m.dur}s ease-out ${m.delay}s infinite`,
                  }}
                />
                <circle
                  cx={m.cx}
                  cy={m.cy}
                  r={m.r}
                  fill={m.color}
                  data-geos-motion
                  style={{
                    animation: `geos-dot-pulse ${m.dur}s ease-in-out ${m.delay}s infinite`,
                  }}
                />
              </g>
            ))}
          </svg>
        </div>
      </div>

      {/* Bottom fade so content never collides with the globe's lower limb. */}
      <div className="absolute inset-x-0 bottom-0 h-40 bg-gradient-to-t from-[#04060b] to-transparent" />
    </div>
  );
}
