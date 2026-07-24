import assert from "node:assert/strict";
import test from "node:test";
import { createCerulClient, normalizeBaseUrl } from "../src/index.js";

test("one generated client accepts cloud and local /v1 base URLs", () => {
  assert.equal(normalizeBaseUrl("https://api.cerul.ai/v1"), "https://api.cerul.ai");
  assert.equal(
    normalizeBaseUrl("http://127.0.0.1:23785/v1/"),
    "http://127.0.0.1:23785",
  );
});

test("generated search operation uses the normalized path and token", async () => {
  let captured: Request | undefined;
  const client = createCerulClient({
    baseUrl: "http://127.0.0.1:23785/v1",
    token: "installation_test",
    fetch: async (input, init) => {
      captured = new Request(input, init);
      return Response.json({
        request_id: "req_test1",
        execution: { location: "local" },
        usage: { billable: false, quantity: 1, unit: "query", credits: 0 },
        warnings: [],
        data: [],
      });
    },
  });
  const result = await client.POST("/v1/search", {
    body: {
      query: "evidence",
      scope: { library_ids: [], asset_ids: [] },
      execution_policy: "local_only",
      limit: 5,
    },
  });
  assert.equal(result.error, undefined);
  assert.equal(captured?.url, "http://127.0.0.1:23785/v1/search");
  assert.equal(captured?.headers.get("authorization"), "Bearer installation_test");
});
