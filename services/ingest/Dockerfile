# syntax=docker/dockerfile:1
# CyberGuard ingest service (SPEC-004). Multi-stage; Node 22 LTS alpine.
#
# BUILD CONTEXT = repo ROOT (see infra/dev/docker-compose.dev.yml). The pnpm
# workspace lockfile (pnpm-lock.yaml), the workspace manifest (pnpm-workspace.yaml)
# and the detection rules (rules/, ADR-0012 driver) all live at the repo root,
# OUTSIDE services/ingest — so a services/ingest-scoped context cannot reach them.

FROM node:22-alpine AS build
WORKDIR /repo
RUN corepack enable
# Resolve the ingest package's dependency closure from the workspace lockfile.
COPY pnpm-workspace.yaml package.json pnpm-lock.yaml ./
COPY services/ingest/package.json services/ingest/package.json
RUN pnpm install --frozen-lockfile --filter @cyberguard/ingest...
# Build, then extract a self-contained deployable (package + flattened prod deps)
# so the runtime stage carries a plain node_modules instead of workspace symlinks.
COPY services/ingest services/ingest
RUN pnpm --filter @cyberguard/ingest run build \
 && pnpm --filter @cyberguard/ingest deploy --legacy --prod /deploy

FROM node:22-alpine AS runtime
WORKDIR /app
ENV NODE_ENV=production
COPY --from=build /deploy/package.json ./package.json
COPY --from=build /deploy/node_modules ./node_modules
# dist comes straight from tsc's flat output (tsconfig.build.json,
# rootDir=src). /deploy supplies only package.json + prod node_modules,
# so what code ships never depends on which files pnpm deploy selects.
COPY --from=build /repo/services/ingest/dist ./dist
# ADR-0012 Amendment 2026-06-07 — bundle the Sigma detection rules into the image
# so the production driver's INGEST_DETECT_RULES_DIR default resolves: the process
# runs with cwd = /app (WORKDIR), so <cwd>/rules/windows = /app/rules/windows.
COPY rules ./rules
EXPOSE 8080 8443
# Runs as root in the dev image so it can write the self-generated server TLS
# identity to the /certs volume; non-root hardening is a deploy concern.
CMD ["node", "dist/index.js"]
