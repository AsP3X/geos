# Geos frontend: build SPA in Node, serve static assets + /api proxy via nginx.
# Build context is the repo root (pnpm workspace + lockfile).
FROM node:22-bookworm-slim AS build
RUN corepack enable
WORKDIR /app

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY frontend ./frontend
RUN pnpm install --frozen-lockfile
# Empty base URL → browser uses same-origin /api (proxied by nginx below).
ARG VITE_API_BASE_URL=
ENV VITE_API_BASE_URL=$VITE_API_BASE_URL
RUN pnpm --filter geos-frontend build

FROM nginx:1.27-alpine AS runtime
COPY docker/frontend/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/frontend/dist /usr/share/nginx/html
EXPOSE 4173
CMD ["nginx", "-g", "daemon off;"]
