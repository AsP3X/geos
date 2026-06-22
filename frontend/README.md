# Geos frontend

React + Vite + TypeScript + Tailwind CSS v4 + shadcn/ui. Renders the Geos
command-center UI: a react-three-fiber 3D globe with sidebars, fuzzy + semantic
search, and live event updates over WebSocket.

## Structure

| Path | Purpose |
| --- | --- |
| `src/main.tsx` | App bootstrap (React root). |
| `src/App.tsx` | Root component. |
| `src/index.css` | Tailwind v4 entry + amber-on-black theme tokens (shadcn-compatible). |
| `src/lib/utils.ts` | `cn()` class-merge helper. |
| `components.json` | shadcn/ui config; add components with `pnpm dlx shadcn@latest add <name>`. |

## Environment variables

| Variable | Purpose | Default |
| --- | --- | --- |
| `VITE_API_BASE_URL` | Base URL for the Geos API (`/api/v1`). | `http://localhost:8080` |

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
