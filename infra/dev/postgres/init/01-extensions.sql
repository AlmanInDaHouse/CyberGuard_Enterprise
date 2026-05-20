-- CyberGuard — Postgres init script.
-- Loads the pgvector extension on first container boot.
-- See ADR-0003 Rule 1 (pgvector enabled day-one).

CREATE EXTENSION IF NOT EXISTS vector;
