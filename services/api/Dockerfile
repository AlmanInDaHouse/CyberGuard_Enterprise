# syntax=docker/dockerfile:1
# CyberGuard human-facing API / auth-core (SPEC-008). Multi-stage; Node 22 LTS
# alpine. Mirrors services/ingest/Dockerfile. User-facing only — no agent mTLS.

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
EXPOSE 8081
CMD ["node", "dist/index.js"]
