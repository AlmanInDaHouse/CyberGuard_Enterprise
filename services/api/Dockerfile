# syntax=docker/dockerfile:1
# CyberGuard human-facing API / auth-core (SPEC-008). Multi-stage; Node 22 LTS
# alpine. Mirrors services/ingest/Dockerfile. User-facing only — no agent mTLS.
#
# BUILD CONTEXT = repo ROOT (see infra/dev/docker-compose.dev.yml). The pnpm
# workspace lockfile (pnpm-lock.yaml) and the workspace manifest
# (pnpm-workspace.yaml) live at the repo root, OUTSIDE services/api — so a
# services/api-scoped context cannot reach them.

FROM node:22-alpine AS build
WORKDIR /repo
RUN corepack enable
# Resolve the api package's dependency closure from the workspace lockfile.
COPY pnpm-workspace.yaml package.json pnpm-lock.yaml ./
COPY services/api/package.json services/api/package.json
RUN pnpm install --frozen-lockfile --filter @cyberguard/api...
# Build, then extract a self-contained deployable (package + flattened prod deps)
# so the runtime stage carries a plain node_modules instead of workspace symlinks.
COPY services/api services/api
RUN pnpm --filter @cyberguard/api run build \
 && pnpm --filter @cyberguard/api deploy --legacy --prod /deploy

FROM node:22-alpine AS runtime
WORKDIR /app
ENV NODE_ENV=production
COPY --from=build /deploy/package.json ./package.json
COPY --from=build /deploy/node_modules ./node_modules
# dist comes straight from tsc's flat output (tsconfig.build.json,
# rootDir=src). /deploy supplies only package.json + prod node_modules,
# so what code ships never depends on which files pnpm deploy selects.
COPY --from=build /repo/services/api/dist ./dist
EXPOSE 8081
CMD ["node", "dist/index.js"]
