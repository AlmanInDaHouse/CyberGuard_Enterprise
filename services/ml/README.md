# services/ml

Python FastAPI service hosting pre-trained models (sentence-transformers, quantized instruct models served by llama.cpp / vLLM).

Populated by SPEC-XXX-ml. Until then this folder is a placeholder.

Expected responsibilities:

- Score events against pre-trained anomaly and classification models.
- Emit alerts with `detection_source: "ml"` directly into the alert bus.
- Provide embedding generation endpoints for downstream retrieval (pgvector).
- Surface a stable, versioned HTTP contract; the rest of the platform must remain language-agnostic.

This is the only Python surface in the project. The boundary is intentional: ML model serving is the one workload where the Python ecosystem outweighs the cost of an additional toolchain.
