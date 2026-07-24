import assert from "node:assert/strict";
import test from "node:test";
import { capabilityToTool } from "../src/index.js";

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
