# Cerul TypeScript SDK

The `cerul` package is generated from Cerul's public OpenAPI projection and
works with both local and cloud runtimes.

```ts
import { createCerulClient } from "cerul";

const client = createCerulClient({
  baseUrl: "https://api.cerul.ai/v1",
  token: process.env.CERUL_API_KEY,
});

const { data, error } = await client.POST("/v1/search", {
  body: {
    query: "grounded evidence",
    scope: { library_ids: ["library_demo1"], asset_ids: [] },
    execution_policy: "prefer_local",
  },
});
```

For local use, pass the loopback `/v1` URL and installation token. The client
normalizes the terminal `/v1` exactly once.
