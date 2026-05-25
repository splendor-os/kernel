import { SplendorClient } from "@splendor/client";
import type { AuditAttribution, Percept, RunConfig, WorkOrderAuthorization } from "@splendor/types";

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

const runConfig: RunConfig = {
  trace_db: "./data/trace.db",
  state_db: "./data/state.db",
  tenants: [{ id: tenantId, allowed_actions: [], allowed_adapters: [], allowed_permissions: [] }],
  agents: [{ id: agentId, tenant_id: tenantId, policy: { type: "static", actions: [] } }]
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
  baseUrl: process.env.SPLENDOR_DAEMON_URL ?? "http://127.0.0.1:7347",
  token,
  defaultAudit: audit
});

const created = await client.createRun(runConfig, { workOrder });
await client.appendPercept(created.run_id, agentId, percept, { tenantId });
const traces = await client.readTraces(created.run_id, { redactionPolicy: "tenant-default" });
const stateHead = await client.getStateHead(created.run_id);
const replay = await client.requestReplay(created.run_id);

console.log({ run: created, traces: traces.length, stateHead, replay });
