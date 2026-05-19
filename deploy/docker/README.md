# Docker deployment

Compose manifests and Dockerfiles for local and single-node deployment.

The root-level [`docker-compose.yml`](../../docker-compose.yml) and [`docker-compose.dev.yml`](../../docker-compose.dev.yml) are the canonical entry points. This subfolder holds:

- Service Dockerfiles (when not co-located with each service).
- Compose overrides for specific deployment scenarios.
- Shared init scripts (TLS material generation, MinIO bucket bootstrap, NATS streams).

Populated by SPEC-XXX-infra-docker.
