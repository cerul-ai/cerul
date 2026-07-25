import assert from "node:assert/strict";
import test from "node:test";
import {
  capabilityToTool,
  createMcpBridge,
} from "../src/index.js";

test("advertised capability maps to one constrained MCP tool", () => {
  const tool = capabilityToTool({
    capability_id: "retrieval.search",
    version: "1",
    runtimes: ["local", "cloud"],
    read_only: true,
    billable: false,
    data_egress: false,
    requires_confirmation: false,
    required_scopes: ["libraries:read", "assets:read"],
  });
  assert.equal(tool.name, "cerul_retrieval_search");
  assert.deepEqual(tool.inputSchema.required, [
    "scope",
    "execution_policy",
    "input",
  ]);
  assert.match(tool.description, /data_egress=false/);
});

test("MCP pins the advertised capability version in the created Job", async () => {
  let createdBody: Record<string, unknown> | undefined;
  const capability = {
    capability_id: "asset.index",
    version: "1",
    runtimes: ["cloud"],
    read_only: false,
    billable: true,
    data_egress: true,
    requires_confirmation: true,
    required_scopes: ["assets:read"],
  };
  const bridge = createMcpBridge({
    GET: async () => ({
      data: { data: [capability] },
      error: undefined,
      response: new Response(),
    }),
    POST: async (_path, request) => {
      createdBody = request?.body as Record<string, unknown>;
      return {
        data: { data: { id: "job_1" } },
        error: undefined,
        response: new Response(),
      };
    },
  } as unknown as Parameters<typeof createMcpBridge>[0]);

  await bridge.callTool("cerul_asset_index", {
    scope: { library_ids: ["library_1"], asset_ids: ["asset_1"] },
    execution_policy: "cloud_required",
    input: { asset_id: "asset_1" },
    idempotency_key: "mcp-job-1",
  });
  assert.equal(createdBody?.capability_id, "asset.index");
  assert.equal(createdBody?.capability_version, "1");
});
