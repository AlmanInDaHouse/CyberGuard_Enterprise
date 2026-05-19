# Schemas

Versioned data contracts for CyberGuard.

| Subfolder | Purpose |
|---|---|
| [`cges/`](cges/) | CyberGuard Common Event Schema (OCSF-aligned). |
| [`api/`](api/) | OpenAPI definitions for HTTP surfaces. |

All schemas under this directory are validatable (JSON Schema, OpenAPI). CI fails if a schema is not parseable by the corresponding validator.
