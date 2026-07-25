#!/usr/bin/env node
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { createCerulClient, type CerulClient } from "cerul";

export type Capability = {
  capability_id: string;
  version: string;
  runtimes: string[];
  read_only: boolean;
  billable: boolean;
  data_egress: boolean;
  requires_confirmation: boolean;
  required_scopes: string[];
};

export type Tool = {
  name: string;
  description: string;
  inputSchema: {
    type: "object";
    required: string[];
    properties: Record<string, unknown>;
    additionalProperties: false;
  };
};

export function capabilityToTool(capability: Capability): Tool {
  return {
    name: `cerul_${capability.capability_id.replace(/[^a-zA-Z0-9_]/g, "_")}`,
    description: [
      `${capability.capability_id}@${capability.version}`,
      `runtimes=${capability.runtimes.join(",")}`,
      `billable=${capability.billable}`,
      `data_egress=${capability.data_egress}`,
      `confirmation=${capability.requires_confirmation}`,
      `scopes=${capability.required_scopes.join(",")}`,
    ].join("; "),
    inputSchema: {
      type: "object",
      required: ["scope", "execution_policy", "input"],
      additionalProperties: false,
      properties: {
        scope: {
          type: "object",
          required: ["library_ids", "asset_ids"],
          properties: {
            library_ids: { type: "array", items: { type: "string" } },
            asset_ids: { type: "array", items: { type: "string" } },
          },
        },
        execution_policy: {
          type: "string",
          enum: ["local_only", "prefer_local", "cloud_allowed", "cloud_required"],
        },
        input: { type: "object", additionalProperties: true },
        idempotency_key: { type: "string", minLength: 1 },
      },
    },
  };
}

export function createMcpBridge(client: CerulClient) {
  async function capabilities(): Promise<Capability[]> {
    const response = await client.GET("/v1/capabilities");
    if (response.error || !response.data) {
      throw new Error("Cerul capability discovery failed");
    }
    return response.data.data as Capability[];
  }

  return {
    async listTools() {
      return (await capabilities()).map(capabilityToTool);
    },
    async callTool(name: string, argumentsValue: Record<string, unknown>) {
      const advertised = await capabilities();
      const capability = advertised.find(
        (candidate) => capabilityToTool(candidate).name === name,
      );
      if (!capability) throw new Error(`unknown or unavailable capability: ${name}`);
      const idempotencyKey =
        typeof argumentsValue.idempotency_key === "string"
          ? argumentsValue.idempotency_key
          : crypto.randomUUID();
      const response = await client.POST("/v1/jobs", {
        params: { header: { "Idempotency-Key": idempotencyKey } },
        body: {
          capability_id: capability.capability_id,
          capability_version: capability.version,
          scope: argumentsValue.scope as {
            library_ids: string[];
            asset_ids: string[];
          },
          execution_policy: argumentsValue.execution_policy as
            | "local_only"
            | "prefer_local"
            | "cloud_allowed"
            | "cloud_required",
          input: argumentsValue.input as Record<string, unknown>,
        },
      });
      if (response.error || !response.data) {
        throw new Error(`Cerul job creation failed for ${capability.capability_id}`);
      }
      return {
        content: [
          {
            type: "text",
            text: JSON.stringify(response.data),
          },
        ],
        structuredContent: response.data,
      };
    },
  };
}

async function runStdio() {
  const bridge = createMcpBridge(
    createCerulClient({
      baseUrl: process.env.CERUL_BASE_URL,
      token:
        process.env.CERUL_API_KEY ?? process.env.CERUL_INSTALLATION_TOKEN,
    }),
  );
  const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
  for await (const line of lines) {
    if (!line.trim()) continue;
    const request = JSON.parse(line) as {
      jsonrpc: "2.0";
      id?: string | number;
      method: string;
      params?: Record<string, unknown>;
    };
    if (request.id === undefined) continue;
    try {
      let result: unknown;
      if (request.method === "initialize") {
        result = {
          protocolVersion: "2025-06-18",
          capabilities: { tools: { listChanged: true } },
          serverInfo: { name: "cerul", version: "0.0.0" },
        };
      } else if (request.method === "tools/list") {
        result = { tools: await bridge.listTools() };
      } else if (request.method === "tools/call") {
        result = await bridge.callTool(
          String(request.params?.name ?? ""),
          (request.params?.arguments ?? {}) as Record<string, unknown>,
        );
      } else {
        throw new Error(`method not found: ${request.method}`);
      }
      process.stdout.write(
        `${JSON.stringify({ jsonrpc: "2.0", id: request.id, result })}\n`,
      );
    } catch (error) {
      process.stdout.write(
        `${JSON.stringify({
          jsonrpc: "2.0",
          id: request.id,
          error: {
            code: -32000,
            message: error instanceof Error ? error.message : "MCP request failed",
          },
        })}\n`,
      );
    }
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await runStdio();
}
