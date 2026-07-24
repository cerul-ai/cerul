# Compatibility

The same generated clients target both Cerul runtimes:

| Runtime | Base URL | Authentication |
|---|---|---|
| Cloud | `https://api.cerul.ai/v1` | Workspace API key or OAuth access token |
| Local | `http://127.0.0.1:<port>/v1` | Per-installation loopback token |

Clients discover runtime support through `GET /v1/capabilities`. An unavailable
capability returns the standard `capability_unavailable` error. Local and cloud
responses share the same OpenAPI schemas, but availability, execution location,
egress, billing, and latency may differ.

The earlier `cerul-js`, `cerul-python`, `cerul-cli`, and `cerul-plugin-cc`
repositories are migration sources. Their package names remain compatible;
their maintained source location is now this repository.
