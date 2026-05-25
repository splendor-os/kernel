import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { ACTION_STATUS_VALUES, CANONICAL_SCHEMA_FIELDS, TRACE_EVENT_KIND_VARIANTS } from "@splendor/types";

const repoRoot = process.cwd();

function readRepoFile(path: string): string {
  return readFileSync(join(repoRoot, path), "utf8");
}

function extractStructFields(source: string, name: string): string[] {
  const match = new RegExp(`(?:pub\\s+)?struct\\s+${name}\\s*\\{([\\s\\S]*?)\\n\\}`, "m").exec(source);
  assert.ok(match, `struct ${name} must exist`);
  return Array.from(match[1].matchAll(/^\s*(?:pub\s+)?([a-z][A-Za-z0-9_]*)\s*:/gm), (field) => field[1]);
}

function extractEnumVariants(source: string, name: string): string[] {
  const enumStart = source.indexOf(`enum ${name}`);
  assert.notEqual(enumStart, -1, `enum ${name} must exist`);
  const open = source.indexOf("{", enumStart);
  assert.notEqual(open, -1, `enum ${name} must have a body`);
  let depth = 0;
  for (let index = open; index < source.length; index += 1) {
    const character = source[index];
    if (character === "{") depth += 1;
    if (character === "}") depth -= 1;
    if (depth === 0) {
      const body = source.slice(open + 1, index);
      return Array.from(body.matchAll(/^    ([A-Z][A-Za-z0-9]+)(?:\s*[,\{])/gm), (variant) => variant[1]);
    }
  }
  throw new Error(`enum ${name} closing brace not found`);
}

test("TypeScript primitive field contracts match canonical Rust structs", () => {
  const message = readRepoFile("crates/splendor-types/src/message.rs");
  const primitives = readRepoFile("crates/splendor-types/src/primitives.rs");
  const trace = readRepoFile("crates/splendor-types/src/trace.rs");
  const gateway = readRepoFile("crates/splendor-gateway/src/lib.rs");
  const cli = readRepoFile("crates/splendorctl/src/main.rs");

  assert.deepEqual(CANONICAL_SCHEMA_FIELDS.message, extractStructFields(message, "Message"));
  assert.deepEqual(CANONICAL_SCHEMA_FIELDS.percept, extractStructFields(primitives, "Percept"));
  assert.deepEqual(CANONICAL_SCHEMA_FIELDS.trace_event, extractStructFields(trace, "TraceEvent"));
  assert.deepEqual(CANONICAL_SCHEMA_FIELDS.action_request, extractStructFields(gateway, "ActionRequest"));
  assert.deepEqual(CANONICAL_SCHEMA_FIELDS.action_outcome, extractStructFields(gateway, "ActionOutcome"));
  assert.deepEqual(CANONICAL_SCHEMA_FIELDS.state_head, extractStructFields(cli, "StateHeadOutput"));
  assert.deepEqual(CANONICAL_SCHEMA_FIELDS.run_config, extractStructFields(cli, "RunConfig"));
});

test("TypeScript enum contracts match canonical Rust gateway and trace variants", () => {
  const trace = readRepoFile("crates/splendor-types/src/trace.rs");
  const gateway = readRepoFile("crates/splendor-gateway/src/lib.rs");

  assert.deepEqual([...TRACE_EVENT_KIND_VARIANTS], extractEnumVariants(trace, "TraceEventKind"));
  assert.deepEqual([...ACTION_STATUS_VALUES], extractEnumVariants(gateway, "ActionStatus"));
});
