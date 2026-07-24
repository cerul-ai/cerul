import { createHash } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const failures = [];
const forbiddenRoots = ["frontend", "services", "runtimes", "workers", "db"];
for (const entry of await readdir(root, { withFileTypes: true })) {
  if (entry.isDirectory() && forbiddenRoots.includes(entry.name)) {
    failures.push(`forbidden public implementation directory: ${entry.name}`);
  }
}

const openapiFiles = (await walk(root))
  .map((file) => path.relative(root, file))
  .filter((file) => /(?:^|\/)openapi\.(?:json|ya?ml)$/i.test(file));
if (openapiFiles.length !== 1 || openapiFiles[0] !== "openapi.json") {
  failures.push(`expected only generated openapi.json; found ${openapiFiles.join(", ")}`);
}

const openapiText = await readFile(path.join(root, "openapi.json"), "utf8");
const openapi = JSON.parse(openapiText);
const manifest = JSON.parse(
  await readFile(path.join(root, "public-surface-manifest.json"), "utf8"),
);
const operationIds = [];
for (const [route, pathItem] of Object.entries(openapi.paths ?? {})) {
  if (route.startsWith("/v1/admin") || route.startsWith("/api/auth")) {
    failures.push(`internal route leaked: ${route}`);
  }
  for (const operation of Object.values(pathItem)) {
    if (operation?.operationId) operationIds.push(operation.operationId);
  }
}
if (
  JSON.stringify(operationIds.toSorted()) !==
  JSON.stringify([...manifest.operation_ids].toSorted())
) {
  failures.push("manifest operation IDs do not match openapi.json");
}
if (sha256(openapiText) !== manifest.public_openapi_sha256) {
  failures.push("public OpenAPI hash does not match the generated manifest");
}
if (
  /x-cerul-visibility|Stripe|Hyperdrive|provider.secret|BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY/i.test(
    openapiText,
  )
) {
  failures.push("public OpenAPI contains an internal or secret marker");
}

const generatedPython = await readFile(
  path.join(root, "packages", "python", "cerul", "generated_contract.py"),
  "utf8",
);
if (!generatedPython.includes(manifest.public_openapi_sha256)) {
  failures.push("Python operation registry is not from the current contract");
}
const generatedTypescript = await readFile(
  path.join(root, "packages", "js", "src", "generated", "schema.ts"),
  "utf8",
);
for (const operationId of manifest.operation_ids) {
  if (!generatedTypescript.includes(operationId)) {
    failures.push(`TypeScript schema is missing ${operationId}`);
  }
}
for (const [fixtureName, expectedHash] of Object.entries(manifest.fixtures ?? {})) {
  const fixture = await readFile(
    path.join(root, "examples", "fixtures", fixtureName),
    "utf8",
  );
  if (sha256(fixture) !== expectedHash) {
    failures.push(`fixture is not from the current private source: ${fixtureName}`);
  }
}

if (failures.length > 0) {
  console.error("Public boundary check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log(
  `Public boundary check passed (${manifest.operation_ids.length} operations).`,
);

async function walk(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if ([".git", "node_modules", "dist", "target", ".venv"].includes(entry.name)) {
      continue;
    }
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await walk(absolute)));
    else files.push(absolute);
  }
  return files;
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}
