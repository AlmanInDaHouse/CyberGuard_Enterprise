# CyberGuard ingest service (SPEC-004; the image of SPEC-023 §Data contracts §2).
# Multi-stage; Node 22 LTS alpine, pinned by digest.
#
# BUILD CONTEXT = repo ROOT (see infra/dev/docker-compose.dev.yml and deploy-ci.yml). The
# pnpm workspace lockfile (pnpm-lock.yaml), the workspace manifest (pnpm-workspace.yaml),
# the detection rules (rules/, ADR-0012 driver), the playbooks and LICENSE/NOTICE all live
# at the repo root, OUTSIDE services/ingest — so a services/ingest-scoped context cannot
# reach them.

FROM node:22.23.4-alpine3.24@sha256:fe4f6d74e20370acb04ad889077d14d0f64a9c8ea6a42bc60cddf1e844699914 AS build
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
# The image's third-party notices (SPEC-023 Amendment 2026-10-26, item 2), from the
# packages it ships.
COPY .github/scripts/third-party-notices.mjs /notices/
COPY .github/scripts/licence-texts /notices/licence-texts
RUN node /notices/third-party-notices.mjs --image ingest \
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
      org.opencontainers.image.title="CyberGuard ingest" \
      org.opencontainers.image.description="CyberGuard's ingest service: agent enrollment and telemetry, detection and response." \
      org.opencontainers.image.url="https://github.com/AlmanInDaHouse/CyberGuard_Enterprise" \
      org.opencontainers.image.documentation="https://github.com/AlmanInDaHouse/CyberGuard_Enterprise/tree/${CG_REVISION}/docs/operations" \
      org.opencontainers.image.vendor="The CyberGuard Authors"
WORKDIR /app
ENV NODE_ENV=production
# Everything under /app is owned by root and not writable by the process (uid 1000): the
# code, the dependencies, the rules and the playbooks (debt #109).
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
# SPEC-022 §Operational §1 — bundle the SOAR playbooks and their schema, so that
# INGEST_SOAR_PLAYBOOKS_DIR's default (<cwd>/playbooks) and the schema the loader
# compiles (<cwd>/schemas/playbooks/v0.1/playbook.json) resolve in the image.
COPY playbooks ./playbooks
COPY schemas/playbooks ./schemas/playbooks
COPY LICENSE NOTICE /usr/share/doc/cyberguard/
COPY --from=build /notices/THIRD-PARTY-NOTICES /usr/share/doc/cyberguard/
# /app/playbooks-none: an empty directory that leaves SOAR idle (CG_SOAR_PLAYBOOKS_DIR,
# ADR-0021). /certs: owned by the process, so that a new named volume mounted there takes
# that ownership and ingest can write its server certificate pair into it.
RUN mkdir /app/playbooks-none /certs \
 && chown 1000:1000 /certs
USER node
EXPOSE 8080 8443
# GET /health on the enrollment listener, over TLS without verifying the certificate
# (SPEC-023 §Data contracts §2). A stack that sets INGEST_ENROLL_TLS=false sets its own.
HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --start-interval=2s --retries=3 \
  CMD ["node", "-e", "require('https').get({host:'127.0.0.1',port:Number(process.env.INGEST_ENROLL_PORT||8080),path:'/health',rejectUnauthorized:false,timeout:4000},(r)=>process.exit(r.statusCode===200?0:1)).on('error',()=>process.exit(1))"]
CMD ["node", "dist/index.js"]
