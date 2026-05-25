import type {
  AgentId,
  AppendPerceptResponse,
  AuditAttribution,
  CreateRunResponse,
  Percept,
  ReplayRequest,
  ReplayResponse,
  RunConfig,
  RunId,
  StateHead,
  TenantId,
  TraceEvent,
  WorkOrderAuthorization
} from "@splendor/types";

export type FetchLike = (input: string | URL | Request, init?: RequestInit) => Promise<Response>;

export interface SplendorClientOptions {
  /** Runtime daemon base URL, for example `http://127.0.0.1:7347`. */
  baseUrl: string;
  /** Caller bearer token. The client never silently falls back to anonymous calls. */
  token: string;
  /** Optional fetch implementation for tests or controlled runtimes. */
  fetch?: FetchLike;
  /** Daemon API version header. Defaults to the 0.02-dev compatibility line. */
  apiVersion?: string;
  /** Optional default audit attribution for mutating calls. */
  defaultAudit?: AuditAttribution;
}

export interface CreateRunOptions {
  workOrder: WorkOrderAuthorization;
  audit?: AuditAttribution;
}

export interface AppendPerceptOptions {
  tenantId?: TenantId;
  audit?: AuditAttribution;
}

export interface ReadTracesOptions {
  /** Required by the daemon security boundary before raw trace data is exposed. */
  redactionPolicy: string;
  afterSequence?: number;
  limit?: number;
}

export interface RequestReplayOptions extends ReplayRequest {}

export interface DaemonErrorPayload {
  code?: string;
  message?: string;
  details?: unknown;
  error?: {
    code?: string;
    message?: string;
    details?: unknown;
  };
}

export class SplendorClientError extends Error {
  readonly status: number;
  readonly code: string;
  readonly details: unknown;
  readonly requestId?: string;
  readonly responseBody?: unknown;

  constructor(params: {
    status: number;
    code: string;
    message: string;
    details?: unknown;
    requestId?: string;
    responseBody?: unknown;
  }) {
    super(params.message);
    this.name = "SplendorClientError";
    this.status = params.status;
    this.code = params.code;
    this.details = params.details;
    this.requestId = params.requestId;
    this.responseBody = params.responseBody;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

export class SplendorClient {
  private readonly baseUrl: string;
  private readonly token: string;
  private readonly fetcher: FetchLike;
  private readonly apiVersion: string;
  private readonly defaultAudit?: AuditAttribution;

  constructor(options: SplendorClientOptions) {
    if (!options.baseUrl.trim()) {
      throw new TypeError("SplendorClient requires a daemon baseUrl");
    }
    if (!options.token.trim()) {
      throw new TypeError("SplendorClient requires an authenticated caller token; unauthenticated fallback is not allowed");
    }
    this.baseUrl = options.baseUrl.endsWith("/") ? options.baseUrl : `${options.baseUrl}/`;
    this.token = options.token;
    this.fetcher = options.fetch ?? globalThis.fetch?.bind(globalThis);
    if (!this.fetcher) {
      throw new TypeError("SplendorClient requires a fetch implementation in this runtime");
    }
    this.apiVersion = options.apiVersion ?? "0.02-dev";
    this.defaultAudit = options.defaultAudit;
  }

  async createRun(config: RunConfig, options: CreateRunOptions): Promise<CreateRunResponse> {
    if (!options?.workOrder) {
      throw new TypeError("createRun requires a signed, scoped work order authorization");
    }
    this.validateCreateRunWorkOrder(options.workOrder);
    const audit = this.requireAudit(options.audit);
    return this.request<CreateRunResponse>("POST", "runs", {
      body: {
        run_config: config,
        work_order: options.workOrder,
        audit
      }
    });
  }

  async appendPercept(
    runId: RunId,
    agentId: AgentId,
    percept: Percept,
    options: AppendPerceptOptions = {}
  ): Promise<AppendPerceptResponse> {
    const audit = this.requireAudit(options.audit);
    return this.request<AppendPerceptResponse>("POST", `runs/${encodeURIComponent(runId)}/percepts`, {
      body: {
        agent_id: agentId,
        percept,
        tenant_id: options.tenantId,
        audit
      }
    });
  }

  async readTraces(runId: RunId, options: ReadTracesOptions): Promise<TraceEvent[]> {
    if (!options?.redactionPolicy.trim()) {
      throw new TypeError("readTraces requires an explicit redactionPolicy");
    }
    const payload = await this.request<TraceEvent[] | { events: TraceEvent[] }>("GET", `runs/${encodeURIComponent(runId)}/traces`, {
      query: {
        redaction_policy: options.redactionPolicy,
        after_sequence: options.afterSequence,
        limit: options.limit
      }
    });
    return Array.isArray(payload) ? payload : payload.events;
  }

  async *streamTraces(runId: RunId, options: ReadTracesOptions): AsyncIterable<TraceEvent> {
    for (const event of await this.readTraces(runId, options)) {
      yield event;
    }
  }

  async getStateHead(runId: RunId): Promise<StateHead> {
    return this.request<StateHead>("GET", `runs/${encodeURIComponent(runId)}/state-head`);
  }

  async requestReplay(runId: RunId, options: RequestReplayOptions = {}): Promise<ReplayResponse> {
    return this.request<ReplayResponse>("POST", `runs/${encodeURIComponent(runId)}/replay`, {
      body: {
        mode: options.mode ?? "inspect_only",
        from_snapshot: options.from_snapshot ?? null,
        include_state: options.include_state ?? false
      }
    });
  }

  private requireAudit(audit?: AuditAttribution): AuditAttribution {
    const attribution = audit ?? this.defaultAudit;
    if (!attribution) {
      throw new TypeError("mutating daemon calls require audit attribution");
    }
    return attribution;
  }

  private validateCreateRunWorkOrder(workOrder: WorkOrderAuthorization): void {
    if (!workOrder.signature?.key_id.trim() || !workOrder.signature.signature.trim()) {
      throw new TypeError("createRun requires signed work order signature metadata");
    }
    if (!workOrder.allowed_scopes.includes("runs_create")) {
      throw new TypeError("createRun work order must allow the runs_create scope");
    }
    if (workOrder.revocation !== "active") {
      throw new TypeError("createRun work order must not be revoked");
    }
    const expiresAt = Date.parse(workOrder.expires_at);
    if (Number.isNaN(expiresAt) || expiresAt <= Date.now()) {
      throw new TypeError("createRun work order must have a future expires_at timestamp");
    }
  }

  private buildUrl(path: string, query?: Record<string, string | number | boolean | undefined>): URL {
    const url = new URL(path, this.baseUrl);
    if (query) {
      for (const [key, value] of Object.entries(query)) {
        if (value !== undefined) {
          url.searchParams.set(key, String(value));
        }
      }
    }
    return url;
  }

  private async request<T>(
    method: string,
    path: string,
    options: { body?: unknown; query?: Record<string, string | number | boolean | undefined> } = {}
  ): Promise<T> {
    const url = this.buildUrl(path, options.query);
    const headers = new Headers({
      Accept: "application/json",
      Authorization: `Bearer ${this.token}`,
      "X-Splendor-API-Version": this.apiVersion,
      "X-Splendor-Client": "@splendor/client"
    });
    let body: string | undefined;
    if (options.body !== undefined) {
      headers.set("Content-Type", "application/json");
      body = JSON.stringify(options.body);
    }

    let response: Response;
    try {
      response = await this.fetcher(url, { method, headers, body });
    } catch (error) {
      throw new SplendorClientError({
        status: 0,
        code: "network_error",
        message: "Daemon request failed before a response was received",
        details: { cause: error instanceof Error ? error.message : String(error) }
      });
    }

    if (!response.ok) {
      throw await this.toClientError(response);
    }

    if (response.status === 204) {
      return undefined as T;
    }
    const text = await response.text();
    if (!text.trim()) {
      return undefined as T;
    }
    try {
      return JSON.parse(text) as T;
    } catch (error) {
      throw new SplendorClientError({
        status: response.status,
        code: "invalid_json",
        message: "Daemon returned a non-JSON response",
        details: { cause: error instanceof Error ? error.message : String(error), body: text },
        requestId: response.headers.get("x-request-id") ?? response.headers.get("x-correlation-id") ?? undefined,
        responseBody: text
      });
    }
  }

  private async toClientError(response: Response): Promise<SplendorClientError> {
    const requestId = response.headers.get("x-request-id") ?? response.headers.get("x-correlation-id") ?? undefined;
    const text = await response.text();
    let payload: DaemonErrorPayload | string = text;
    if (text.trim()) {
      try {
        payload = JSON.parse(text) as DaemonErrorPayload;
      } catch {
        payload = text;
      }
    }

    if (typeof payload === "object" && payload !== null) {
      const nested = payload.error;
      const code = nested?.code ?? payload.code ?? `http_${response.status}`;
      const message = nested?.message ?? payload.message ?? response.statusText;
      const details = nested?.details ?? payload.details ?? payload;
      return new SplendorClientError({
        status: response.status,
        code,
        message,
        details,
        requestId,
        responseBody: payload
      });
    }

    return new SplendorClientError({
      status: response.status,
      code: `http_${response.status}`,
      message: response.statusText || "Daemon request failed",
      details: { body: payload },
      requestId,
      responseBody: payload
    });
  }
}
