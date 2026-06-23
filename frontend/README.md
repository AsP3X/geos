# Geos frontend

React + Vite + TypeScript + Tailwind CSS v4 + shadcn/ui. Renders the Geos
command-center UI: a react-three-fiber 3D globe with sidebars, fuzzy + semantic
search, and live event updates over WebSocket.

## Structure

| Path | Purpose |
| --- | --- |
| `src/main.tsx` | App bootstrap (React root). |
| `src/App.tsx` | Router: login, register, command center. |
| `src/auth/context.tsx` | Auth session provider (JWT + refresh). |
| `src/hooks/useAuth.ts` | Auth hook for pages. |
| `src/hooks/useEventStream.ts` | WebSocket live event subscription. |
| `src/lib/api-client.ts` | REST client for auth and protected routes. |
| `src/lib/events-api.ts` | Events list, search, stream URL helpers. |
| `src/pages/` | Login, register, command center UI. |
| `src/components/globe/GlobeViewport.tsx` | 2D globe scaffold (r3f later). |
| `src/index.css` | Tailwind v4 entry + amber-on-black theme tokens (shadcn-compatible). |
| `src/lib/utils.ts` | `cn()` class-merge helper. |
| `components.json` | shadcn/ui config; add components with `pnpm dlx shadcn@latest add <name>`. |

## Environment variables

| Variable | Purpose | Default |
| --- | --- | --- |
| `VITE_API_BASE_URL` | Optional absolute API URL baked in at build time (local `pnpm dev`). Leave unset for same-origin `/api`. | _(empty — use proxy)_ |
| `VITE_API_PROXY_TARGET` | Backend URL for the Vite dev server `/api` proxy only (`pnpm dev`). | `http://localhost:8080` |
| `GEOS_API_BASE_URL` | **Docker/runtime:** public API URL injected into `/config.js` at container start (no rebuild). Leave empty for same-origin `/api` behind NPM. | _(empty)_ |

Copy the root `.env.example` and adjust as needed; Vite only exposes variables
prefixed with `VITE_`.

## Commands

Run from the repo root (pnpm workspace) or this directory:

```bash
pnpm install            # install workspace deps
pnpm --filter geos-frontend dev        # start dev server (http://localhost:5173)
pnpm --filter geos-frontend build      # type-check + production build
pnpm --filter geos-frontend lint       # eslint
pnpm --filter geos-frontend typecheck  # tsc --noEmit
```
