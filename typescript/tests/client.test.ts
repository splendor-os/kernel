import assert from "node:assert/strict";
import test from "node:test";
import { SplendorClient, SplendorClientError, type FetchLike } from "@splendor/client";
import type {
  Action,
  AuditAttribution,
  CreateRunRequest,
  LifecycleRequest,
  Percept,
  SubmitActionRequest,
  TraceRecord,
  WorkOrderAuthorization
} from "@splendor/types";

const tenantId = "00000000-0000-0000-0000-000000000001";
const agentId = "00000000-0000-0000-0000-000000000002";
const runId = "00000000-0000-0000-0000-000000000003";

const audit: AuditAttribution = {
  principal: {
    app: { app_principal_id: "app.test", label: "test app" },
    client_principal_id: "client.test",
    label: "test client"
  },
  credential_id: "cred_test",
  requested_at: "2026-05-25T00:00:00Z"
};

const workOrder: WorkOrderAuthorization = {
  work_order_id: "wo_test",
  tenant_id: tenantId,
  agent_id: agentId,
  run_id: null,
  allowed_scopes: ["runs_create"],
  signature: { key_id: "test-key", signature: "test-signature" },
  expires_at: new Date(Date.now() + 60 * 60 * 1000).toISOString(),
  revocation: "active"
};

const createRunRequest: CreateRunRequest = {
  tenant_id: tenantId,
  agent_id: agentId,
  work_order: workOrder,
  credential: null,
  audit_attribution: audit,
  allowed_actions: ["noop"],
  allowed_adapters: ["daemon.local"],
  allowed_permissions: [],
  policy_actions: [],
  policy_bundle_required: false,
  policy_bundle: null,
  registered_actions: [],
  approval_policies: [],
  allowed_percept_schemas: ["splendor.percept.test.v1"],
  allowed_percept_sources: ["daemon-client-local"],
  initial_state: { seed: true },
  snapshot_interval: 1
};

const lifecycleRequest: LifecycleRequest = {
  credential: null,
  work_order: null,
  audit_attribution: audit,
  reason: "test",
  approval_evidence: null
};

const action: Action = {
  name: "noop",
  params: { ok: true },
  side_effect_class: "External",
  cost_estimate: null,
  required_permissions: [],
  preconditions: [],
  postconditions: []
};

const percept: Percept = {
  schema: "splendor.percept.test.v1",
  payload: { ok: true },
  provenance: { source: "test", detail: null },
  timestamp: "2026-05-25T00:00:00Z"
};

function makeFetch(responseBody: unknown, status = 200, headers: Record<string, string> = {}): {
  fetcher: FetchLike;
  calls: Array<{ url: string; init: RequestInit; jsonBody: unknown }>;
} {
  const calls: Array<{ url: string; init: RequestInit; jsonBody: unknown }> = [];
  const fetcher: FetchLike = async (input, init = {}) => {
    const rawBody = typeof init.body === "string" ? init.body : undefined;
    calls.push({
      url: String(input),
      init,
      jsonBody: rawBody ? JSON.parse(rawBody) : undefined
    });
    return new Response(responseBody === undefined ? undefined : JSON.stringify(responseBody), {
      status,
      statusText: status >= 400 ? "Forbidden" : "OK",
      headers: { "content-type": "application/json", ...headers }
    });
  };
  return { fetcher, calls };
}

function makeRawFetch(body: string | undefined, status = 200, headers: Record<string, string> = {}): FetchLike {
  return async () => new Response(body, { status, statusText: status >= 400 ? "Failure" : "OK", headers });
}

test("client refuses unauthenticated fallback", () => {
  assert.throws(
    () => new SplendorClient({ baseUrl: "http://127.0.0.1:8077", token: "   " }),
    /authenticated caller token/
  );
  assert.throws(() => new SplendorClient({ baseUrl: "   ", token: "token" }), /baseUrl/);
});

test("client reports unavailable fetch implementation", () => {
  const originalFetch = globalThis.fetch;
  try {
    Object.defineProperty(globalThis, "fetch", { configurable: true, value: undefined });
    assert.throws(() => new SplendorClient({ baseUrl: "http://127.0.0.1:8077", token: "token" }), /fetch implementation/);
  } finally {
    Object.defineProperty(globalThis, "fetch", { configurable: true, value: originalFetch });
  }
});

test("createRun posts run config with work-order authorization and audit attribution", async () => {
  const { fetcher, calls } = makeFetch({ run_id: runId, status: "created" });
  const client = new SplendorClient({ baseUrl: "https://daemon.example/v1", token: "token", fetch: fetcher });

  const response = await client.createRun(createRunRequest);

  assert.equal(response.run_id, runId);
  assert.equal(calls.length, 1);
  assert.equal(new URL(calls[0].url).pathname, "/v1/runs");
  assert.equal(calls[0].init.method, "POST");
  const headers = new Headers(calls[0].init.headers);
  assert.equal(headers.get("authorization"), "Bearer token");
  assert.equal(headers.get("x-splendor-api-version"), "0.02-dev");
  assert.deepEqual(calls[0].jsonBody, createRunRequest);
});

test("createRun fails closed when work order or audit attribution is absent", async () => {
  const { fetcher } = makeFetch({ run_id: runId });
  const client = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: fetcher });

  await assert.rejects(() => client.createRun({ ...createRunRequest, audit_attribution: null }), /audit attribution/);
  await assert.rejects(() => client.createRun({ ...createRunRequest, work_order: null as never }), /work order/);
});

test("lifecycle and inspection helpers use daemon endpoint shapes", async () => {
  const calls: Array<{ url: string; init: RequestInit; jsonBody: unknown }> = [];
  const tick = { run_id: runId, status: "running", tick_id: 1, state_node_id: "state_1", action_outcomes: [] };
  const inspected = {
    run_id: runId,
    tenant_id: tenantId,
    agent_id: agentId,
    status: "paused",
    state_head: "state_1",
    ticks: 1,
    adapter_executions: 0,
    created_at: "2026-05-25T00:00:00Z",
    updated_at: "2026-05-25T00:00:01Z"
  };
  const fetcher: FetchLike = async (input, init = {}) => {
    const rawBody = typeof init.body === "string" ? init.body : undefined;
    calls.push({ url: String(input), init, jsonBody: rawBody ? JSON.parse(rawBody) : undefined });
    const pathname = new URL(String(input)).pathname;
    const body = pathname.endsWith("/start") || pathname.endsWith("/resume") ? tick : inspected;
    return new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } });
  };
  const client = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: fetcher });

  assert.equal((await client.inspectRun(runId)).run_id, runId);
  assert.equal((await client.startRun(runId, lifecycleRequest)).state_node_id, "state_1");
  assert.equal((await client.pauseRun(runId, lifecycleRequest)).status, "paused");
  assert.equal((await client.resumeRun(runId, lifecycleRequest)).tick_id, 1);
  assert.equal((await client.stopRun(runId, lifecycleRequest)).state_head, "state_1");

  assert.deepEqual(
    calls.map((call) => [new URL(call.url).pathname, call.init.method]),
    [
      [`/runs/${runId}`, "GET"],
      [`/runs/${runId}/start`, "POST"],
      [`/runs/${runId}/pause`, "POST"],
      [`/runs/${runId}/resume`, "POST"],
      [`/runs/${runId}/stop`, "POST"]
    ]
  );
  assert.deepEqual(calls[1].jsonBody, lifecycleRequest);
});

test("createRun rejects structurally invalid work-order authority before daemon calls", async () => {
  const { fetcher, calls } = makeFetch({ run_id: runId });
  const client = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: fetcher });

  await assert.rejects(
    () => client.createRun({ ...createRunRequest, work_order: { ...workOrder, signature: null } }),
    /signature/
  );
  await assert.rejects(
    () => client.createRun({ ...createRunRequest, work_order: { ...workOrder, signature: { key_id: "", signature: "" } } }),
    /signature/
  );
  await assert.rejects(
    () => client.createRun({ ...createRunRequest, work_order: { ...workOrder, allowed_scopes: [] } }),
    /runs_create/
  );
  await assert.rejects(
    () =>
      client.createRun({ ...createRunRequest, work_order: { ...workOrder, revocation: { revoked: { reason: "operator" } } } }),
    /revoked/
  );
  await assert.rejects(
    () =>
      client.createRun({ ...createRunRequest, work_order: { ...workOrder, expires_at: new Date(Date.now() - 1000).toISOString() } }),
    /future expires_at/
  );
  await assert.rejects(
    () => client.createRun({ ...createRunRequest, work_order: { ...workOrder, expires_at: "not-a-date" } }),
    /future expires_at/
  );
  assert.equal(calls.length, 0);
});

test("appendPercept posts a run-scoped percept with audit attribution", async () => {
  const { fetcher, calls } = makeFetch({ run_id: runId, accepted: 1 });
  const client = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: fetcher });

  const response = await client.appendPercept(runId, percept, { audit });

  assert.equal(response.accepted, 1);
  assert.equal(new URL(calls[0].url).pathname, `/runs/${runId}/percepts`);
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(calls[0].jsonBody, { credential: null, audit_attribution: audit, percept });
});

test("submitAction stays trace-linked and audit-attributed", async () => {
  const outcome = {
    action_id: "00000000-0000-0000-0000-000000000010",
    status: "Denied",
    verification: { allowed: false, reasons: ["action_not_allowed"], artifacts: null },
    post_verification: null,
    output: null,
    error: null,
    completed_at: "2026-05-25T00:00:00Z"
  };
  const { fetcher, calls } = makeFetch(outcome);
  const client = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: fetcher });
  const request: SubmitActionRequest = {
    run_id: runId,
    tenant_id: tenantId,
    agent_id: agentId,
    credential: null,
    audit_attribution: audit,
    causal_trace_id: "00000000-0000-0000-0000-000000000011",
    action,
    adapter: "daemon.local",
    quota_usage: null,
    satisfied_preconditions: [],
    approval_evidence: null
  };

  assert.equal((await client.submitAction(request)).status, "Denied");
  assert.equal(new URL(calls[0].url).pathname, "/actions");
  assert.deepEqual(calls[0].jsonBody, request);
  await assert.rejects(() => client.submitAction({ ...request, causal_trace_id: null }), /trace linkage/);
  await assert.rejects(() => client.submitAction({ ...request, audit_attribution: null }), /audit attribution/);
});

test("readTraces requires redaction policy and preserves event order", async () => {
  const records: TraceRecord[] = [
    { run_id: runId, sequence: 1, recorded_at: "2026-05-25T00:00:00Z", event_hash: { algorithm: "Blake3", value: "h1" }, prev_event_hash: null, payload: { trace_id: "00000000-0000-0000-0000-000000000010", run_id: runId, sequence: 1, timestamp: "2026-05-25T00:00:00Z", kind: "RunStarted" } },
    {
      run_id: runId,
      sequence: 2,
      recorded_at: "2026-05-25T00:00:01Z",
      event_hash: { algorithm: "Blake3", value: "h2" },
      prev_event_hash: { algorithm: "Blake3", value: "h1" },
      payload: { trace_id: "00000000-0000-0000-0000-000000000011", run_id: runId, sequence: 2, timestamp: "2026-05-25T00:00:01Z", kind: { LoopTickStarted: { tick_id: 1 } } }
    }
  ];
  const { fetcher, calls } = makeFetch({ run_id: runId, records });
  const client = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: fetcher });

  await assert.rejects(() => client.readTraces(runId, { redactionPolicy: " " }), /redactionPolicy/);
  const result = await client.readTraces(runId, { redactionPolicy: "tenant-default", start: 1, end: 3 });

  assert.deepEqual(result.map((event) => event.sequence), [1, 2]);
  const url = new URL(calls[0].url);
  assert.equal(url.pathname, `/runs/${runId}/traces`);
  assert.equal(url.searchParams.get("redaction_policy"), "tenant-default");
  assert.equal(url.searchParams.get("start"), "1");
  assert.equal(url.searchParams.get("end"), "3");
});

test("streamTraces exposes an async iterable over trace reads", async () => {
  const records: TraceRecord[] = [
    { run_id: runId, sequence: 1, recorded_at: "2026-05-25T00:00:00Z", event_hash: { algorithm: "Blake3", value: "h1" }, prev_event_hash: null, payload: { trace_id: "00000000-0000-0000-0000-000000000010", run_id: runId, sequence: 1, timestamp: "2026-05-25T00:00:00Z", kind: "RunStarted" } }
  ];
  const { fetcher } = makeFetch({ run_id: runId, records });
  const client = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: fetcher });

  const streamed: TraceRecord[] = [];
  for await (const record of client.streamTraces(runId, { redactionPolicy: "tenant-default" })) {
    streamed.push(record);
  }

  assert.deepEqual(streamed, records);
});

test("getStateHead and requestReplay call daemon inspection endpoints", async () => {
  const stateFetch = makeFetch({ run_id: runId, state_node_id: "state_1", parent_state_node_ids: [], data_hash: "blake3:abc", created_at: "2026-05-25T00:00:00Z", label: null });
  const stateClient = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: stateFetch.fetcher });

  const head = await stateClient.getStateHead(runId);

  assert.equal(head.state_node_id, "state_1");
  assert.equal(new URL(stateFetch.calls[0].url).pathname, `/runs/${runId}/state-head`);

  const replayFetch = makeFetch({ replay_id: "replay_test", run_id: runId, mode: "inspect_only", event_count: 3, action_event_count: 0, approval_events: [] });
  const replayClient = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: replayFetch.fetcher });

  const replay = await replayClient.requestReplay(runId);

  assert.equal(replay.replay_id, "replay_test");
  assert.equal(new URL(replayFetch.calls[0].url).pathname, `/runs/${runId}/replay`);
  assert.deepEqual(replayFetch.calls[0].jsonBody, { credential: null });
});

test("daemon errors preserve status, daemon code, details, and request id", async () => {
  const { fetcher } = makeFetch(
    { code: "missing_scope", message: "Missing endpoint scope", details: { required: "splendor.traces.read" } },
    403,
    { "x-request-id": "req_123" }
  );
  const client = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: fetcher });

  await assert.rejects(
    () => client.readTraces(runId, { redactionPolicy: "tenant-default" }),
    (error: unknown) => {
      assert.ok(error instanceof SplendorClientError);
      assert.equal(error.status, 403);
      assert.equal(error.code, "missing_scope");
      assert.equal(error.message, "Missing endpoint scope");
      assert.deepEqual(error.details, { required: "splendor.traces.read" });
      assert.equal(error.requestId, "req_123");
      return true;
    }
  );
});

test("network and malformed daemon responses become structured client errors", async () => {
  const networkClient = new SplendorClient({
    baseUrl: "https://daemon.example",
    token: "token",
    fetch: async () => {
      throw new Error("connection refused");
    }
  });

  await assert.rejects(
    () => networkClient.getStateHead(runId),
    (error: unknown) => {
      assert.ok(error instanceof SplendorClientError);
      assert.equal(error.status, 0);
      assert.equal(error.code, "network_error");
      assert.deepEqual(error.details, { cause: "connection refused" });
      return true;
    }
  );

  const invalidJsonClient = new SplendorClient({
    baseUrl: "https://daemon.example",
    token: "token",
    fetch: makeRawFetch("not-json", 200, { "x-correlation-id": "corr_1" })
  });

  await assert.rejects(
    () => invalidJsonClient.getStateHead(runId),
    (error: unknown) => {
      assert.ok(error instanceof SplendorClientError);
      assert.equal(error.code, "invalid_json");
      assert.equal(error.requestId, "corr_1");
      assert.equal(error.responseBody, "not-json");
      return true;
    }
  );
});

test("empty success responses and text error responses remain explicit", async () => {
  const noContentClient = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: makeRawFetch(undefined, 204) });
  assert.equal(await noContentClient.getStateHead(runId), undefined);

  const emptyBodyClient = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: makeRawFetch("", 200) });
  assert.equal(await emptyBodyClient.getStateHead(runId), undefined);

  const textErrorClient = new SplendorClient({
    baseUrl: "https://daemon.example",
    token: "token",
    fetch: makeRawFetch("plain failure", 500, { "x-request-id": "req_text" })
  });

  await assert.rejects(
    () => textErrorClient.getStateHead(runId),
    (error: unknown) => {
      assert.ok(error instanceof SplendorClientError);
      assert.equal(error.status, 500);
      assert.equal(error.code, "http_500");
      assert.equal(error.message, "Failure");
      assert.deepEqual(error.details, { body: "plain failure" });
      assert.equal(error.requestId, "req_text");
      return true;
    }
  );

  const malformedErrorClient = new SplendorClient({
    baseUrl: "https://daemon.example",
    token: "token",
    fetch: makeRawFetch("{not-json", 400)
  });
  await assert.rejects(malformedErrorClient.getStateHead(runId), { code: "http_400" });
});
