import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { fileURLToPath } from "node:url";
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

test("generated client consumers can parse the canonical contract fixtures", async () => {
  const fixtureUrl = new URL("../../../examples/fixtures/", import.meta.url);
  const artifact = JSON.parse(
    await readFile(
      fileURLToPath(new URL("artifact-response.json", fixtureUrl)),
      "utf8",
    ),
  );
  const response = JSON.parse(
    await readFile(
      fileURLToPath(new URL("response-envelope.json", fixtureUrl)),
      "utf8",
    ),
  );
  const upload = JSON.parse(
    await readFile(
      fileURLToPath(new URL("upload-response.json", fixtureUrl)),
      "utf8",
    ),
  );

  assert.equal(artifact.data.id, "artifact_fixture1");
  assert.equal(artifact.execution.location, "local");
  assert.equal(response.data.status, "completed");
  assert.equal(response.data.citations[0].id, "ev_fixture1");
  assert.equal(upload.data.asset_id, "asset_fixture1");
  assert.notEqual(upload.data.id, upload.data.asset_id);
});
