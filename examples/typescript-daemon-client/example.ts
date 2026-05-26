import { SplendorClient } from "@splendor/client";
import type { AuditAttribution, CreateRunRequest, LifecycleRequest, Percept, WorkOrderAuthorization } from "@splendor/types";

const tenantId = "00000000-0000-0000-0000-000000000001";
const agentId = "00000000-0000-0000-0000-000000000002";

const audit: AuditAttribution = {
  principal: {
    app: { app_principal_id: "app.example", label: "TypeScript example" },
    client_principal_id: "client.example",
    label: "Local developer client"
  },
  credential_id: "cred_example",
  requested_at: new Date().toISOString()
};

const workOrder: WorkOrderAuthorization = {
  work_order_id: "wo_example",
  tenant_id: tenantId,
  agent_id: agentId,
  run_id: null,
  allowed_scopes: ["runs_create"],
  signature: { key_id: "example-key", signature: "example-signature" },
  expires_at: new Date(Date.now() + 60 * 60 * 1000).toISOString(),
  revocation: "active"
};

const createRunRequest: CreateRunRequest = {
  tenant_id: tenantId,
  agent_id: agentId,
  work_order: workOrder,
  credential: null,
  audit_attribution: audit,
  allowed_actions: [],
  allowed_adapters: ["daemon.local"],
  allowed_permissions: [],
  policy_actions: [],
  registered_actions: [],
  allowed_percept_schemas: ["splendor.percept.example.v1"],
  allowed_percept_sources: ["typescript-example"],
  initial_state: { example: true },
  snapshot_interval: 1
};

const lifecycle: LifecycleRequest = {
  credential: null,
  work_order: null,
  audit_attribution: audit,
  reason: "typescript daemon client example"
};

const percept: Percept = {
  schema: "splendor.percept.example.v1",
  payload: { message: "hello from TypeScript" },
  provenance: { source: "typescript-example", detail: null },
  timestamp: new Date().toISOString()
};

const token = process.env.SPLENDOR_TOKEN;
if (!token) {
  throw new Error("SPLENDOR_TOKEN is required; the TypeScript client never falls back to anonymous daemon calls");
}

const client = new SplendorClient({
  baseUrl: process.env.SPLENDOR_DAEMON_URL ?? "http://127.0.0.1:8077",
  token,
  defaultAudit: audit
});

const created = await client.createRun(createRunRequest);
await client.appendPercept(created.run_id, percept);
const tick = await client.startRun(created.run_id, lifecycle);
const traces = await client.readTraces(created.run_id, { redactionPolicy: "tenant-default" });
const stateHead = await client.getStateHead(created.run_id);
const replay = await client.requestReplay(created.run_id);

console.log({ run: created, tick, traces: traces.length, stateHead, replay });
