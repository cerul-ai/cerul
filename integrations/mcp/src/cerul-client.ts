import createClient, { type Client } from "openapi-fetch";
import type { paths } from "./generated/schema.js";

export type CerulClient = Client<paths>;

export type CerulClientOptions = {
  baseUrl?: string;
  token?: string;
  fetch?: typeof fetch;
};

export function createCerulClient(
  options: CerulClientOptions = {},
): CerulClient {
  const token = options.token ?? readEnvironmentToken();
  return createClient<paths>({
    baseUrl: normalizeBaseUrl(options.baseUrl ?? "https://api.cerul.ai"),
    fetch: options.fetch,
    headers: token
      ? {
          Authorization: `Bearer ${token}`,
          "X-Cerul-Client-Source": "mcp",
        }
      : {
          "X-Cerul-Client-Source": "mcp",
        },
  });
}

export function normalizeBaseUrl(value: string): string {
  const url = new URL(value);
  url.pathname = url.pathname.replace(/\/+$/, "").replace(/\/v1$/, "");
  url.search = "";
  url.hash = "";
  return url.toString().replace(/\/$/, "");
}

function readEnvironmentToken(): string | undefined {
  const processLike = globalThis as typeof globalThis & {
    process?: { env?: Record<string, string | undefined> };
  };
  return (
    processLike.process?.env?.CERUL_API_KEY ??
    processLike.process?.env?.CERUL_INSTALLATION_TOKEN
  );
}
