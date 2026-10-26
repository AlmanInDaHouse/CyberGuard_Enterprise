# CyberGuard human-facing API / auth-core (SPEC-008; the image of SPEC-023 §Data contracts
# §2). Multi-stage; Node 22 LTS alpine, pinned by digest. Mirrors services/ingest/Dockerfile.
# User-facing only — no agent mTLS.
#
# BUILD CONTEXT = repo ROOT (see infra/dev/docker-compose.dev.yml and deploy-ci.yml). The
# pnpm workspace lockfile (pnpm-lock.yaml), the workspace manifest (pnpm-workspace.yaml)
# and LICENSE/NOTICE live at the repo root, OUTSIDE services/api — so a services/api-scoped
# context cannot reach them.

FROM node:22.23.4-alpine3.24@sha256:fe4f6d74e20370acb04ad889077d14d0f64a9c8ea6a42bc60cddf1e844699914 AS build
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
# The image's third-party notices (SPEC-023 Amendment 2026-10-26, item 2), from the
# packages it ships.
COPY .github/scripts/third-party-notices.mjs /notices/
COPY .github/scripts/licence-texts /notices/licence-texts
RUN node /notices/third-party-notices.mjs --image api \
      --base "node:22.23.4-alpine3.24@sha256:fe4f6d74e20370acb04ad889077d14d0f64a9c8ea6a42bc60cddf1e844699914" \
      --base-software "Node.js, npm, Yarn, Corepack and the Alpine Linux packages" \
      --modules /deploy/node_modules --out /notices/THIRD-PARTY-NOTICES

FROM node:22.23.4-alpine3.24@sha256:fe4f6d74e20370acb04ad889077d14d0f64a9c8ea6a42bc60cddf1e844699914 AS runtime
ARG CG_VERSION=dev
ARG CG_REVISION=unknown
LABEL org.opencontainers.image.source="https://github.com/AlmanInDaHouse/CyberGuard_Enterprise" \
      org.opencontainers.image.revision="${CG_REVISION}" \
      org.opencontainers.image.version="${CG_VERSION}" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.title="CyberGuard api" \
      org.opencontainers.image.description="CyberGuard's API: sign-in, incidents, the forensic export and report, and their verifier." \
      org.opencontainers.image.url="https://github.com/AlmanInDaHouse/CyberGuard_Enterprise" \
      org.opencontainers.image.documentation="https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/tree/${CG_REVISION}/docs/operations" \
      org.opencontainers.image.vendor="The CyberGuard Authors"
WORKDIR /app
ENV NODE_ENV=production
# Owned by root and not writable by the process (uid 1000).
COPY --from=build /deploy/package.json ./package.json
COPY --from=build /deploy/node_modules ./node_modules
# dist comes straight from tsc's flat output (tsconfig.build.json,
# rootDir=src). /deploy supplies only package.json + prod node_modules,
# so what code ships never depends on which files pnpm deploy selects.
COPY --from=build /repo/services/api/dist ./dist
COPY LICENSE NOTICE /usr/share/doc/cyberguard/
COPY --from=build /notices/THIRD-PARTY-NOTICES /usr/share/doc/cyberguard/
USER node
EXPOSE 8081
# GET /health (SPEC-008 as SPEC-023 amends it), which touches no store.
HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --start-interval=2s --retries=3 \
  CMD ["node", "-e", "require('http').get({host:'127.0.0.1',port:Number(process.env.API_PORT||8081),path:'/health',timeout:4000},(r)=>process.exit(r.statusCode===200?0:1)).on('error',()=>process.exit(1))"]
CMD ["node", "dist/index.js"]
