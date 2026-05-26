/**
 * Schema-aligned TypeScript types for Splendor 0.02-dev daemon and runtime
 * primitives. These types intentionally contain no kernel execution logic.
 */

export type ISODateTime = string;
export type TenantId = string;
export type AgentId = string;
export type RunId = string;
export type MessageId = string;
export type TraceId = string;
export type ActionId = string;

export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonObject | JsonValue[];
export interface JsonObject {
  readonly [key: string]: JsonValue;
}

export type HashAlgorithm = "Blake3";

export interface ContentHash {
  algorithm: HashAlgorithm;
  value: string;
}

export type SnapshotId = ContentHash;

export interface PerceptProvenance {
  source: string;
  detail: string | null;
}

export interface Percept {
  schema: string;
  payload: JsonValue;
  provenance: PerceptProvenance;
  timestamp: ISODateTime;
}

export type SideEffectClass = "ReadOnly" | "Filesystem" | "Network" | "External" | { Custom: string };

export interface CostEstimate {
  units: string;
  amount: number;
}

export interface Action {
  name: string;
  params: JsonValue;
  side_effect_class: SideEffectClass;
  cost_estimate: CostEstimate | null;
  required_permissions: string[];
  preconditions: string[];
  postconditions: string[];
}

export interface QuotaUsage {
  actions: number;
  action_duration_ms: number;
  filesystem_read_bytes: number;
  filesystem_write_bytes: number;
  network_read_bytes: number;
  network_write_bytes: number;
  http_requests: number;
}

export type ConstraintKind = "Hard" | "Soft";
export type ConstraintScope = "Global" | "Action" | "State";

export interface Constraint {
  id: string;
  kind: ConstraintKind;
  scope: ConstraintScope;
  predicate: string;
  obligation: string | null;
}

export interface VerificationResult {
  allowed: boolean;
  reasons: string[];
  artifacts: JsonValue;
}

export interface Feedback {
  kind: string;
  payload: JsonValue;
  recorded_at: ISODateTime;
}

export interface Reward {
  value: number;
  units: string | null;
  recorded_at: ISODateTime;
  context: JsonValue | null;
}

export interface Message {
  message_id: MessageId;
  source_agent_id: AgentId;
  target_agent_id: AgentId;
  run_id: RunId;
  schema: string;
  payload: JsonValue;
  causal_parent: TraceId | null;
  requires_response: boolean;
  created_at: ISODateTime;
}

export type MessageSchemaVersion = "v1";
export type MessageDeliveryStatus = "pending" | "queued" | "delivered" | "rejected" | "expired" | "consumed";

export interface MessageTraceLinks {
  queued_trace_id: TraceId | null;
  delivered_trace_id: TraceId | null;
  rejected_trace_id: TraceId | null;
  expired_trace_id: TraceId | null;
  consumed_trace_id: TraceId | null;
}

export interface MessageEnvelope {
  message: Message;
  schema_version: MessageSchemaVersion;
  delivery_status: MessageDeliveryStatus;
  trace_links: MessageTraceLinks;
}

export interface MessageTraceContext {
  message_id: MessageId;
  source_agent_id: AgentId;
  target_agent_id: AgentId;
  run_id: RunId;
  schema: string;
  causal_parent: TraceId | null;
}

export interface TraceIntegrity {
  prev_event_hash: ContentHash | null;
  event_hash: ContentHash;
}

export type TraceEventKind =
  | "RunStarted"
  | { LoopTickStarted: { tick_id: number } }
  | { PerceptsReceived: { percepts: Percept[] } }
  | { StateLoaded: { state_hash: ContentHash | null } }
  | { PolicyInvoked: { policy: string } }
  | { PolicyCompleted: { policy: string } }
  | { CandidatesProposed: { actions: Action[] } }
  | { ConstraintsEvaluated: { constraints: Constraint[]; result: VerificationResult } }
  | { ActionVerificationStarted: { action: Action } }
  | { ActionVerificationCompleted: { action: Action; result: VerificationResult } }
  | { ActionExecuted: { action: Action; outcome: JsonValue } }
  | { ActionDenied: { action: Action; result: VerificationResult } }
  | { ActionFailed: { action: Action; error: string; result: VerificationResult } }
  | { OutcomeRecorded: { outcome: JsonValue; feedback: Feedback | null; reward: Reward | null } }
  | { StateCommitted: { state_hash: ContentHash; snapshot_id: SnapshotId | null } }
  | { MessageQueued: { message: MessageTraceContext } }
  | { MessageDelivered: { message: MessageTraceContext } }
  | { MessageRejected: { message: MessageTraceContext; reason: string } }
  | { MessageExpired: { message: MessageTraceContext; reason: string | null } }
  | { MessageConsumed: { message: MessageTraceContext } }
  | { LoopTickCompleted: { tick_id: number; integrity: TraceIntegrity | null } };

export interface TraceEvent {
  trace_id: TraceId;
  run_id: RunId;
  sequence: number;
  timestamp: ISODateTime;
  kind: TraceEventKind;
}

export type ActionStatus = "Executed" | "Denied" | "Failed";

export interface ActionRequest {
  action_id: ActionId;
  tenant_id: TenantId;
  agent_id: AgentId;
  action: Action;
  adapter: string | null;
  quota_usage: QuotaUsage;
  satisfied_preconditions: string[];
  requested_at: ISODateTime;
}

export interface ActionOutcome {
  action_id: ActionId;
  status: ActionStatus;
  verification: VerificationResult;
  post_verification: VerificationResult | null;
  output: JsonValue | null;
  error: string | null;
  completed_at: ISODateTime;
}

export interface StateHead {
  run_id: RunId;
  state_hash: ContentHash;
  snapshot_id: SnapshotId | null;
  trace_sequence: number;
}

export interface RunConfig {
  trace_db: string;
  state_db: string;
  run_id?: RunId;
  tick_budget_ms?: number;
  tick_interval_ms?: number;
  cycles?: number;
  tenants: TenantConfig[];
  agents: AgentConfig[];
  adapters?: AdaptersConfig;
}

export interface TenantConfig {
  id: TenantId;
  allowed_actions: string[];
  allowed_adapters: string[];
  allowed_permissions?: string[];
  quotas?: QuotaPolicy;
}

export interface QuotaPolicy {
  max_actions_per_tick?: number;
  max_action_duration_ms?: number;
  max_filesystem_read_bytes?: number;
  max_filesystem_write_bytes?: number;
  max_network_read_bytes?: number;
  max_network_write_bytes?: number;
  max_http_requests_per_minute?: number;
}

export interface AgentConfig {
  id?: AgentId;
  tenant_id: TenantId;
  run_id?: RunId;
  snapshot_interval?: number;
  initial_state?: string;
  resume?: boolean;
  percepts?: PerceptConfig[];
  policy: PolicyConfig;
}

export interface PerceptConfig {
  schema: string;
  payload: JsonValue;
  source: string;
  detail?: string;
}

export type RunConfigSideEffectClass = "read_only" | "filesystem" | "network" | "external" | string;

export interface ActionConfig {
  name: string;
  adapter?: string;
  params: JsonValue;
  side_effect_class?: RunConfigSideEffectClass;
  required_permissions?: string[];
  preconditions?: string[];
  postconditions?: string[];
  usage?: Partial<QuotaUsage>;
  satisfied_preconditions?: string[];
}

export type PolicyConfig =
  | { type: "static"; actions: ActionConfig[]; next_state?: string | null }
  | { type: "increment"; action?: ActionConfig | null };

export interface AdaptersConfig {
  filesystem?: FilesystemAdapterConfig;
  http?: HttpAdapterConfig;
  readonly [adapter: string]: JsonValue | FilesystemAdapterConfig | HttpAdapterConfig | undefined;
}

export interface FilesystemAdapterConfig {
  base_dir: string;
  max_read_bytes?: number;
  max_write_bytes?: number;
  max_list_entries?: number;
}

export interface HttpAdapterConfig {
  allowed_domains: string[];
  allowed_methods?: string[];
  max_request_bytes?: number;
  max_response_bytes?: number;
  timeout_ms?: number;
}

export type EndpointScope =
  | "runs_create"
  | "runs_resume"
  | "percepts_append"
  | "actions_submit"
  | "traces_read"
  | "state_read"
  | "replay_create"
  | "health_read"
  | "capabilities_read";

export interface AppPrincipal {
  app_principal_id: string;
  label: string | null;
}

export interface ClientPrincipal {
  app: AppPrincipal;
  client_principal_id: string;
  label: string | null;
}

export type CredentialBinding = { tenant: { tenant_id: TenantId } } | { fleet: { fleet_id: string } };
export type CredentialAudience =
  | { daemon: { daemon_id: string } }
  | { instance: { instance_id: string } }
  | { fleet: { fleet_id: string } }
  | { central_manager: { manager_id: string } };
export type RevocationStatus = "active" | { revoked: { reason: string } };

export interface CallerCredential {
  credential_id: string;
  principal: ClientPrincipal;
  scopes: EndpointScope[];
  binding: CredentialBinding;
  audience: CredentialAudience;
  expires_at: ISODateTime;
  revocation: RevocationStatus;
}

export interface WorkOrderSignature {
  key_id: string;
  signature: string;
}

export interface WorkOrderAuthorization {
  work_order_id: string;
  tenant_id: TenantId;
  agent_id: AgentId;
  run_id: RunId | null;
  allowed_scopes: EndpointScope[];
  signature: WorkOrderSignature | null;
  expires_at: ISODateTime;
  revocation: RevocationStatus;
}

export interface AuditAttribution {
  principal: ClientPrincipal;
  credential_id: string | null;
  requested_at: ISODateTime;
}

export interface CreateRunRequest {
  run_config: RunConfig;
  work_order: WorkOrderAuthorization;
  audit: AuditAttribution;
}

export interface CreateRunResponse {
  run_id: RunId;
  status?: string;
}

export interface AppendPerceptRequest {
  agent_id: AgentId;
  percept: Percept;
  tenant_id?: TenantId;
  audit: AuditAttribution;
}

export interface AppendPerceptResponse {
  accepted: boolean;
  trace_id?: TraceId;
}

export interface ReplayRequest {
  mode?: "inspect_only";
  from_snapshot?: SnapshotId | null;
  include_state?: boolean;
}

export interface ReplayResponse {
  replay_id: string;
  run_id: RunId;
  mode: "inspect_only";
  status?: string;
}

export const CANONICAL_SCHEMA_FIELDS = {
  message: [
    "message_id",
    "source_agent_id",
    "target_agent_id",
    "run_id",
    "schema",
    "payload",
    "causal_parent",
    "requires_response",
    "created_at"
  ],
  run_config: [
    "trace_db",
    "state_db",
    "run_id",
    "tick_budget_ms",
    "tick_interval_ms",
    "cycles",
    "tenants",
    "agents",
    "adapters"
  ],
  percept: ["schema", "payload", "provenance", "timestamp"],
  action_request: [
    "action_id",
    "tenant_id",
    "agent_id",
    "action",
    "adapter",
    "quota_usage",
    "satisfied_preconditions",
    "requested_at"
  ],
  action_outcome: ["action_id", "status", "verification", "post_verification", "output", "error", "completed_at"],
  trace_event: ["trace_id", "run_id", "sequence", "timestamp", "kind"],
  state_head: ["run_id", "state_hash", "snapshot_id", "trace_sequence"]
} as const satisfies {
  message: readonly (keyof Message)[];
  run_config: readonly (keyof RunConfig)[];
  percept: readonly (keyof Percept)[];
  action_request: readonly (keyof ActionRequest)[];
  action_outcome: readonly (keyof ActionOutcome)[];
  trace_event: readonly (keyof TraceEvent)[];
  state_head: readonly (keyof StateHead)[];
};

export const TRACE_EVENT_KIND_VARIANTS = [
  "RunStarted",
  "LoopTickStarted",
  "PerceptsReceived",
  "StateLoaded",
  "PolicyInvoked",
  "PolicyCompleted",
  "CandidatesProposed",
  "ConstraintsEvaluated",
  "ActionVerificationStarted",
  "ActionVerificationCompleted",
  "ActionExecuted",
  "ActionDenied",
  "ActionFailed",
  "OutcomeRecorded",
  "StateCommitted",
  "MessageQueued",
  "MessageDelivered",
  "MessageRejected",
  "MessageExpired",
  "MessageConsumed",
  "LoopTickCompleted"
] as const;

export const ACTION_STATUS_VALUES = ["Executed", "Denied", "Failed"] as const satisfies readonly ActionStatus[];

export const ENDPOINT_SCOPE_LABELS: Record<EndpointScope, string> = {
  runs_create: "splendor.runs.create",
  runs_resume: "splendor.runs.resume",
  percepts_append: "splendor.percepts.append",
  actions_submit: "splendor.actions.submit",
  traces_read: "splendor.traces.read",
  state_read: "splendor.state.read",
  replay_create: "splendor.replay.create",
  health_read: "splendor.health.read",
  capabilities_read: "splendor.capabilities.read"
};

export const DAEMON_API_COMPATIBILITY = {
  milestone: "0.02-dev",
  sprint: "0.02-S6",
  daemonApiVersion: "0.02-dev",
  schemaSource: "splendor-types Rust crates and docs/reference"
} as const;
