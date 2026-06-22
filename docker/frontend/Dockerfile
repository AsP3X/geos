# Geos frontend build + static preview server.
# Build context is the repo root so the pnpm workspace + lockfile are available.
FROM node:22-bookworm-slim AS build
RUN corepack enable
WORKDIR /app

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY frontend ./frontend
RUN pnpm install --frozen-lockfile
RUN pnpm --filter geos-frontend build

FROM node:22-bookworm-slim AS runtime
RUN corepack enable
WORKDIR /app/frontend
COPY --from=build /app/frontend/package.json ./package.json
COPY --from=build /app/frontend/dist ./dist
EXPOSE 4173
CMD ["pnpm", "exec", "vite", "preview", "--host", "0.0.0.0", "--port", "4173"]
