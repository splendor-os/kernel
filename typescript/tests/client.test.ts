import assert from "node:assert/strict";
import test from "node:test";
import { SplendorClient, SplendorClientError, type FetchLike } from "@splendor/client";
import type { AuditAttribution, Percept, RunConfig, TraceEvent, WorkOrderAuthorization } from "@splendor/types";

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

const runConfig: RunConfig = {
  trace_db: "./data/trace.db",
  state_db: "./data/state.db",
  tenants: [
    {
      id: tenantId,
      allowed_actions: ["noop"],
      allowed_adapters: ["noop"],
      allowed_permissions: []
    }
  ],
  agents: [
    {
      id: agentId,
      tenant_id: tenantId,
      policy: { type: "static", actions: [] }
    }
  ]
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
    () => new SplendorClient({ baseUrl: "http://127.0.0.1:7347", token: "   " }),
    /authenticated caller token/
  );
  assert.throws(() => new SplendorClient({ baseUrl: "   ", token: "token" }), /baseUrl/);
});

test("client reports unavailable fetch implementation", () => {
  const originalFetch = globalThis.fetch;
  try {
    Object.defineProperty(globalThis, "fetch", { configurable: true, value: undefined });
    assert.throws(() => new SplendorClient({ baseUrl: "http://127.0.0.1:7347", token: "token" }), /fetch implementation/);
  } finally {
    Object.defineProperty(globalThis, "fetch", { configurable: true, value: originalFetch });
  }
});

test("createRun posts run config with work-order authorization and audit attribution", async () => {
  const { fetcher, calls } = makeFetch({ run_id: runId, status: "created" });
  const client = new SplendorClient({ baseUrl: "https://daemon.example/v1", token: "token", fetch: fetcher });

  const response = await client.createRun(runConfig, { workOrder, audit });

  assert.equal(response.run_id, runId);
  assert.equal(calls.length, 1);
  assert.equal(new URL(calls[0].url).pathname, "/v1/runs");
  assert.equal(calls[0].init.method, "POST");
  const headers = new Headers(calls[0].init.headers);
  assert.equal(headers.get("authorization"), "Bearer token");
  assert.equal(headers.get("x-splendor-api-version"), "0.02-dev");
  assert.deepEqual(calls[0].jsonBody, { run_config: runConfig, work_order: workOrder, audit });
});

test("createRun fails closed when work order or audit attribution is absent", async () => {
  const { fetcher } = makeFetch({ run_id: runId });
  const client = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: fetcher });

  await assert.rejects(() => client.createRun(runConfig, { workOrder } as never), /audit attribution/);
  await assert.rejects(() => client.createRun(runConfig, { audit } as never), /work order/);
});

test("createRun rejects structurally invalid work-order authority before daemon calls", async () => {
  const { fetcher, calls } = makeFetch({ run_id: runId });
  const client = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: fetcher });

  await assert.rejects(
    () => client.createRun(runConfig, { workOrder: { ...workOrder, signature: null }, audit }),
    /signature/
  );
  await assert.rejects(
    () => client.createRun(runConfig, { workOrder: { ...workOrder, signature: { key_id: "", signature: "" } }, audit }),
    /signature/
  );
  await assert.rejects(
    () => client.createRun(runConfig, { workOrder: { ...workOrder, allowed_scopes: [] }, audit }),
    /runs_create/
  );
  await assert.rejects(
    () =>
      client.createRun(runConfig, {
        workOrder: { ...workOrder, revocation: { revoked: { reason: "operator" } } },
        audit
      }),
    /revoked/
  );
  await assert.rejects(
    () =>
      client.createRun(runConfig, {
        workOrder: { ...workOrder, expires_at: new Date(Date.now() - 1000).toISOString() },
        audit
      }),
    /future expires_at/
  );
  await assert.rejects(
    () => client.createRun(runConfig, { workOrder: { ...workOrder, expires_at: "not-a-date" }, audit }),
    /future expires_at/
  );
  assert.equal(calls.length, 0);
});

test("appendPercept posts a run-scoped percept with audit attribution", async () => {
  const { fetcher, calls } = makeFetch({ accepted: true, trace_id: "00000000-0000-0000-0000-000000000004" });
  const client = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: fetcher });

  const response = await client.appendPercept(runId, agentId, percept, { tenantId, audit });

  assert.equal(response.accepted, true);
  assert.equal(new URL(calls[0].url).pathname, `/runs/${runId}/percepts`);
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(calls[0].jsonBody, { agent_id: agentId, percept, tenant_id: tenantId, audit });
});

test("readTraces requires redaction policy and preserves event order", async () => {
  const events: TraceEvent[] = [
    { trace_id: "00000000-0000-0000-0000-000000000010", run_id: runId, sequence: 1, timestamp: "2026-05-25T00:00:00Z", kind: "RunStarted" },
    {
      trace_id: "00000000-0000-0000-0000-000000000011",
      run_id: runId,
      sequence: 2,
      timestamp: "2026-05-25T00:00:01Z",
      kind: { LoopTickStarted: { tick_id: 1 } }
    }
  ];
  const { fetcher, calls } = makeFetch({ events });
  const client = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: fetcher });

  await assert.rejects(() => client.readTraces(runId, { redactionPolicy: " " }), /redactionPolicy/);
  const result = await client.readTraces(runId, { redactionPolicy: "tenant-default", afterSequence: 1, limit: 10 });

  assert.deepEqual(result.map((event) => event.sequence), [1, 2]);
  const url = new URL(calls[0].url);
  assert.equal(url.pathname, `/runs/${runId}/traces`);
  assert.equal(url.searchParams.get("redaction_policy"), "tenant-default");
  assert.equal(url.searchParams.get("after_sequence"), "1");
  assert.equal(url.searchParams.get("limit"), "10");
});

test("streamTraces exposes an async iterable over trace reads", async () => {
  const events: TraceEvent[] = [
    { trace_id: "00000000-0000-0000-0000-000000000010", run_id: runId, sequence: 1, timestamp: "2026-05-25T00:00:00Z", kind: "RunStarted" }
  ];
  const { fetcher } = makeFetch(events);
  const client = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: fetcher });

  const streamed: TraceEvent[] = [];
  for await (const event of client.streamTraces(runId, { redactionPolicy: "tenant-default" })) {
    streamed.push(event);
  }

  assert.deepEqual(streamed, events);
});

test("getStateHead and requestReplay call daemon inspection endpoints", async () => {
  const stateHash = { algorithm: "Blake3" as const, value: "abc" };
  const stateFetch = makeFetch({ run_id: runId, state_hash: stateHash, snapshot_id: null, trace_sequence: 7 });
  const stateClient = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: stateFetch.fetcher });

  const head = await stateClient.getStateHead(runId);

  assert.deepEqual(head.state_hash, stateHash);
  assert.equal(new URL(stateFetch.calls[0].url).pathname, `/runs/${runId}/state-head`);

  const replayFetch = makeFetch({ replay_id: "replay_test", run_id: runId, mode: "inspect_only", status: "created" });
  const replayClient = new SplendorClient({ baseUrl: "https://daemon.example", token: "token", fetch: replayFetch.fetcher });

  const replay = await replayClient.requestReplay(runId, { include_state: true });

  assert.equal(replay.replay_id, "replay_test");
  assert.equal(new URL(replayFetch.calls[0].url).pathname, `/runs/${runId}/replay`);
  assert.deepEqual(replayFetch.calls[0].jsonBody, { mode: "inspect_only", from_snapshot: null, include_state: true });
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
