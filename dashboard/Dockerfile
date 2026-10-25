# CyberGuard dashboard (SPEC-009; the image of SPEC-023 §Data contracts §2): Next's standalone
# server. Multi-stage; Node 22 LTS alpine, pinned by digest.
#
# BUILD CONTEXT = repo ROOT (see deploy-ci.yml). The pnpm workspace lockfile, the workspace
# manifest and LICENSE/NOTICE live at the repo root, OUTSIDE dashboard/.

FROM node:22.23.4-alpine3.24@sha256:fe4f6d74e20370acb04ad889077d14d0f64a9c8ea6a42bc60cddf1e844699914 AS build
WORKDIR /repo
ENV NEXT_TELEMETRY_DISABLED=1
RUN corepack enable
# Resolve the dashboard's dependencies from the workspace lockfile. Its development
# dependencies are installed too: `next build` type-checks the workspace's TypeScript. The
# api's manifest is there because the dashboard's tests name it as a workspace dependency.
COPY pnpm-workspace.yaml package.json pnpm-lock.yaml ./
COPY dashboard/package.json dashboard/package.json
COPY services/api/package.json services/api/package.json
RUN pnpm install --frozen-lockfile --filter @cyberguard/dashboard
# `next build` with output "standalone", then the static assets copied beside its server
# (dashboard/scripts/standalone-assets.mjs).
COPY dashboard dashboard
RUN pnpm --filter @cyberguard/dashboard run build

FROM node:22.23.4-alpine3.24@sha256:fe4f6d74e20370acb04ad889077d14d0f64a9c8ea6a42bc60cddf1e844699914 AS runtime
ARG CG_VERSION=dev
ARG CG_REVISION=unknown
LABEL org.opencontainers.image.source="https://github.com/AlmanInDaHouse/CyberGuard_Enterprise" \
      org.opencontainers.image.revision="${CG_REVISION}" \
      org.opencontainers.image.version="${CG_VERSION}" \
      org.opencontainers.image.licenses="Apache-2.0"
WORKDIR /app
# HOSTNAME is the address Next's server binds (Docker would otherwise set it to the
# container's id); PORT its port.
ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1 \
    PORT=3000 \
    HOSTNAME=0.0.0.0
# The standalone tree: the server under dashboard/, the traced node_modules beside it. Owned
# by root and not writable by the process (uid 1000); Next's cache, dashboard/.next/cache,
# wants a tmpfs in a read-only container.
COPY --from=build /repo/dashboard/.next/standalone ./
COPY LICENSE NOTICE /usr/share/doc/cyberguard/
USER node
EXPOSE 3000
# GET /api/health, which calls nothing (SPEC-023 §Operational §7).
HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --start-interval=2s --retries=3 \
  CMD ["node", "-e", "require('http').get({host:'127.0.0.1',port:Number(process.env.PORT||3000),path:'/api/health',timeout:4000},(r)=>process.exit(r.statusCode===200?0:1)).on('error',()=>process.exit(1))"]
CMD ["node", "dashboard/server.js"]
