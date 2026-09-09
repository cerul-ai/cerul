# Core and product boundary

Cerul is a single Rust crate with a reusable library and a thin CLI. The library
owns local media processing, OCR, endpoint adapters, episode timelines,
annotations, sidecars, indexing, and retrieval. It receives configuration and
emits structured events without assuming a terminal or process lifecycle.

Sidecars contain authoritative outputs, including embedding vectors. An index
can be rebuilt without calling a model. All stored intervals use half-open,
integer-microsecond episode time. Model-relative time is converted exactly once.
Completed units are recoverable from checkpoints; final files are atomically
published. The workspace admits one writer at a time.

This crate exposes local processing and endpoint clients. It does not implement
a product UI, hosted inference, or HTTP/MCP serving.

Rust types produce public schemas. Legacy platform OpenAPI projections are
retired rather than hand-modified into a second contract.

LanceDB 0.38.0 currently needs its `remote` Cargo feature to compile: its job
error conversion references a feature-gated HTTP error type. Cerul nevertheless
opens only the configured local workspace directory. This compatibility feature
does not enable a cloud connection or implement the M2 server.
