# syntax=docker/dockerfile:1
# CyberGuard ingest service (SPEC-004). Multi-stage; Node 22 LTS alpine.

FROM node:22-alpine AS build
WORKDIR /app
RUN corepack enable
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY tsconfig.json ./
COPY src ./src
RUN pnpm run build

FROM node:22-alpine AS runtime
WORKDIR /app
RUN corepack enable
ENV NODE_ENV=production
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --prod --frozen-lockfile
COPY --from=build /app/dist ./dist
EXPOSE 8080 8443
# Runs as root in the dev image so it can write the self-generated server
# TLS identity to the /certs volume; non-root hardening is a deploy concern.
CMD ["node", "dist/index.js"]
