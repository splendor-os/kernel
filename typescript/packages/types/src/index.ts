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
export type TraceEventId = TraceId;
export type ActionId = string;
export type FleetId = string;
export type NodeId = string;
export type InstanceId = string;
export type TickId = number;
export type StateNodeId = string;
export type WorkOrderId = string;

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

export interface TraceRecord {
  run_id: string;
  sequence: number;
  payload: JsonValue;
  recorded_at: ISODateTime;
  event_hash: ContentHash;
  prev_event_hash: ContentHash | null;
}

export interface LocalDelegationTraceContext {
  parent_run_id: RunId;
  child_run_id: RunId;
  parent_trace_id: TraceId | null;
  request_message_id: MessageId | null;
  response_message_id: MessageId | null;
  source_agent_id: AgentId;
  target_agent_id: AgentId;
  objective: string;
}

export interface TaskFailure {
  code: string;
  reason: string;
  retryable: boolean;
  trace_id: TraceId | null;
}

export type StateReferenceMode = "snapshot_import" | "read_only_reference";

export interface StateHandoffTraceContext {
  handoff_id: string;
  mode: StateReferenceMode;
  tenant_id: TenantId;
  agent_id: AgentId;
  run_id: RunId;
  work_order_id: string;
  source_instance_id: string | null;
  receiver_instance_id: string | null;
  source_state_node_id: string;
  previous_state_node_id: string | null;
  receiver_state_node_id: string | null;
  snapshot_id: SnapshotId | null;
  source_trace_id: TraceId | null;
}

export interface RemoteMessageTraceContext {
  message: MessageTraceContext;
  tenant_id: TenantId;
  source_instance_id: string;
  target_instance_id: string;
  work_order_id: string;
  attempt: number;
  idempotency_key: string | null;
}

export interface TraceIdentityContext {
  fleet_id?: FleetId | null;
  node_id?: NodeId | null;
  instance_id?: InstanceId | null;
  tenant_id?: TenantId | null;
  agent_id?: AgentId | null;
  run_id: RunId;
  tick_id?: TickId | null;
  action_id?: ActionId | null;
  state_node_id?: StateNodeId | null;
  message_id?: MessageId | null;
}

export interface TraceIntegrity {
  prev_event_hash: ContentHash | null;
  event_hash: ContentHash;
}

export type TraceEventKind =
  | "RunStarted"
  | {
      WorkOrderAccepted: {
        work_order_id: WorkOrderId;
        tenant_id: TenantId;
        agent_id: AgentId;
        run_id: RunId | null;
      };
    }
  | {
      WorkOrderRejected: {
        work_order_id: WorkOrderId | null;
        tenant_id: TenantId | null;
        agent_id: AgentId | null;
        run_id: RunId | null;
        reason: string;
      };
    }
  | { RunPaused: { reason: string | null } }
  | { RunResumed: { reason: string | null } }
  | { RunStopped: { reason: string | null } }
  | { PerceptsAppended: { count: number; schemas: string[] } }
  | { DaemonAudit: { endpoint: string; audit: AuditAttribution } }
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
  | { StateHandoffExported: { handoff: StateHandoffTraceContext } }
  | { StateHandoffImported: { handoff: StateHandoffTraceContext } }
  | { StateHandoffImportFailed: { handoff: StateHandoffTraceContext; reason: string } }
  | { ReadOnlyStateReferenced: { handoff: StateHandoffTraceContext } }
  | { MessageQueued: { message: MessageTraceContext } }
  | { MessageDelivered: { message: MessageTraceContext } }
  | { MessageRejected: { message: MessageTraceContext; reason: string } }
  | { MessageExpired: { message: MessageTraceContext; reason: string | null } }
  | { MessageConsumed: { message: MessageTraceContext } }
  | { RemoteMessageSent: { remote_message: RemoteMessageTraceContext } }
  | { RemoteMessageAccepted: { remote_message: RemoteMessageTraceContext } }
  | { RemoteMessageRejected: { remote_message: RemoteMessageTraceContext; reason: string } }
  | { RemoteMessageDelivered: { remote_message: RemoteMessageTraceContext } }
  | { RemoteMessageTimedOut: { remote_message: RemoteMessageTraceContext; reason: string } }
  | { RemoteMessageDuplicate: { remote_message: RemoteMessageTraceContext; reason: string } }
  | { RemoteMessageTransportFailed: { remote_message: RemoteMessageTraceContext; reason: string } }
  | { DelegationRequested: { delegation: LocalDelegationTraceContext } }
  | { DelegationRejected: { delegation: LocalDelegationTraceContext; reason: string } }
  | { ParentRunCancelled: { parent_run_id: RunId; agent_id: AgentId; reason: string } }
  | { ChildRunStarted: { delegation: LocalDelegationTraceContext } }
  | { ChildRunCompleted: { delegation: LocalDelegationTraceContext } }
  | { ChildRunFailed: { delegation: LocalDelegationTraceContext; failure: TaskFailure } }
  | {
      ChildRunLinked: {
        parent_run_id: RunId;
        child_run_id: RunId;
        parent_agent_id: AgentId;
        child_agent_id: AgentId;
        causal_parent: TraceId | null;
        source_message_id: MessageId | null;
      };
    }
  | { LoopTickCompleted: { tick_id: number; integrity: TraceIntegrity | null } };

export interface TraceEvent {
  trace_event_id: TraceEventId;
  run_id: RunId;
  sequence: number;
  timestamp: ISODateTime;
  identity: TraceIdentityContext;
  kind: TraceEventKind;
}

export type ActionStatus = "Executed" | "Denied" | "Failed";

export interface ActionRequest {
  action_id: ActionId;
  tenant_id: TenantId;
  agent_id: AgentId;
  run_id: RunId;
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
  state_node_id: string;
  parent_state_node_ids: string[];
  data_hash: string;
  created_at: ISODateTime;
  label: string | null;
}

export type RunStatus = "created" | "running" | "paused" | "stopped" | "failed";

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

export interface DaemonActionCandidate {
  action: Action;
  adapter: string | null;
  quota_usage: QuotaUsage | null;
  satisfied_preconditions: string[];
}

export interface RegisteredAction {
  name: string;
  adapter: string;
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
  | "runs_start"
  | "runs_read"
  | "runs_pause"
  | "runs_resume"
  | "runs_stop"
  | "percepts_append"
  | "actions_submit"
  | "traces_read"
  | "state_read"
  | "replay_create"
  | "messages_send"
  | "health_read"
  | "capabilities_read"
  | "nodes_register"
  | "instances_register"
  | "nodes_heartbeat"
  | "instances_heartbeat";

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
  tenant_id: TenantId;
  agent_id: AgentId;
  work_order: WorkOrderAuthorization;
  credential: CallerCredential | null;
  audit_attribution: AuditAttribution | null;
  allowed_actions: string[];
  allowed_adapters: string[];
  allowed_permissions: string[];
  policy_actions: DaemonActionCandidate[];
  registered_actions: RegisteredAction[];
  allowed_percept_schemas: string[];
  allowed_percept_sources: string[];
  initial_state: JsonValue | null;
  snapshot_interval: number | null;
}

export interface CreateRunResponse {
  run_id: RunId;
  status: RunStatus;
}

export interface LifecycleRequest {
  credential: CallerCredential | null;
  work_order: WorkOrderAuthorization | null;
  audit_attribution: AuditAttribution | null;
  reason: string | null;
}

export interface RunInspectResponse {
  run_id: RunId;
  tenant_id: TenantId;
  agent_id: AgentId;
  status: RunStatus;
  state_head: string | null;
  ticks: number;
  adapter_executions: number;
  created_at: ISODateTime;
  updated_at: ISODateTime;
}

export interface TickResponse {
  run_id: RunId;
  status: RunStatus;
  tick_id: number;
  state_node_id: string;
  action_outcomes: ActionOutcome[];
}

export interface AppendPerceptRequest {
  credential: CallerCredential | null;
  audit_attribution: AuditAttribution | null;
  percept: Percept | null;
}

export interface AppendPerceptResponse {
  run_id: RunId;
  accepted: number;
}

export interface ReplayRequest {
  credential: CallerCredential | null;
}

export interface ReplayResponse {
  replay_id: string;
  run_id: RunId;
  mode: string;
  event_count: number;
  action_event_count: number;
}

export interface TracePageResponse {
  run_id: RunId;
  records: TraceRecord[];
}

export interface SubmitActionRequest {
  run_id: RunId;
  tenant_id: TenantId;
  agent_id: AgentId;
  credential: CallerCredential | null;
  audit_attribution: AuditAttribution | null;
  causal_trace_id: TraceId | null;
  action: Action;
  adapter: string | null;
  quota_usage: QuotaUsage | null;
  satisfied_preconditions: string[];
}

export interface HealthResponse {
  status: string;
  local_only: boolean;
  runtime_available: boolean;
}

export interface CapabilitiesResponse {
  daemon_api_version: string;
  local_only: boolean;
  replay_modes: string[];
  endpoints: string[];
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
    "run_id",
    "action",
    "adapter",
    "quota_usage",
    "satisfied_preconditions",
    "requested_at"
  ],
  action_outcome: ["action_id", "status", "verification", "post_verification", "output", "error", "completed_at"],
  trace_event: ["trace_event_id", "run_id", "sequence", "timestamp", "identity", "kind"],
  state_head: ["run_id", "state_node_id", "parent_state_node_ids", "data_hash", "created_at", "label"],
  create_run_request: [
    "tenant_id",
    "agent_id",
    "work_order",
    "credential",
    "audit_attribution",
    "allowed_actions",
    "allowed_adapters",
    "allowed_permissions",
    "policy_actions",
    "registered_actions",
    "allowed_percept_schemas",
    "allowed_percept_sources",
    "initial_state",
    "snapshot_interval"
  ],
  lifecycle_request: ["credential", "work_order", "audit_attribution", "reason"],
  run_inspect_response: [
    "run_id",
    "tenant_id",
    "agent_id",
    "status",
    "state_head",
    "ticks",
    "adapter_executions",
    "created_at",
    "updated_at"
  ],
  tick_response: ["run_id", "status", "tick_id", "state_node_id", "action_outcomes"],
  append_percept_request: ["credential", "audit_attribution", "percept"],
  trace_page_response: ["run_id", "records"],
  replay_response: ["replay_id", "run_id", "mode", "event_count", "action_event_count"],
  submit_action_request: [
    "run_id",
    "tenant_id",
    "agent_id",
    "credential",
    "audit_attribution",
    "causal_trace_id",
    "action",
    "adapter",
    "quota_usage",
    "satisfied_preconditions"
  ],
  health_response: ["status", "local_only", "runtime_available"],
  capabilities_response: ["daemon_api_version", "local_only", "replay_modes", "endpoints"]
} as const satisfies {
  message: readonly (keyof Message)[];
  run_config: readonly (keyof RunConfig)[];
  percept: readonly (keyof Percept)[];
  action_request: readonly (keyof ActionRequest)[];
  action_outcome: readonly (keyof ActionOutcome)[];
  trace_event: readonly (keyof TraceEvent)[];
  state_head: readonly (keyof StateHead)[];
  create_run_request: readonly (keyof CreateRunRequest)[];
  lifecycle_request: readonly (keyof LifecycleRequest)[];
  run_inspect_response: readonly (keyof RunInspectResponse)[];
  tick_response: readonly (keyof TickResponse)[];
  append_percept_request: readonly (keyof AppendPerceptRequest)[];
  trace_page_response: readonly (keyof TracePageResponse)[];
  replay_response: readonly (keyof ReplayResponse)[];
  submit_action_request: readonly (keyof SubmitActionRequest)[];
  health_response: readonly (keyof HealthResponse)[];
  capabilities_response: readonly (keyof CapabilitiesResponse)[];
};

export const TRACE_EVENT_KIND_VARIANTS = [
  "RunStarted",
  "WorkOrderAccepted",
  "WorkOrderRejected",
  "RunPaused",
  "RunResumed",
  "RunStopped",
  "PerceptsAppended",
  "DaemonAudit",
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
  "StateHandoffExported",
  "StateHandoffImported",
  "StateHandoffImportFailed",
  "ReadOnlyStateReferenced",
  "MessageQueued",
  "MessageDelivered",
  "MessageRejected",
  "MessageExpired",
  "MessageConsumed",
  "RemoteMessageSent",
  "RemoteMessageAccepted",
  "RemoteMessageRejected",
  "RemoteMessageDelivered",
  "RemoteMessageTimedOut",
  "RemoteMessageDuplicate",
  "RemoteMessageTransportFailed",
  "DelegationRequested",
  "DelegationRejected",
  "ParentRunCancelled",
  "ChildRunStarted",
  "ChildRunCompleted",
  "ChildRunFailed",
  "ChildRunLinked",
  "LoopTickCompleted"
] as const;

export const ACTION_STATUS_VALUES = ["Executed", "Denied", "Failed"] as const satisfies readonly ActionStatus[];

export const ENDPOINT_SCOPE_VALUES = [
  "RunsCreate",
  "RunsStart",
  "RunsRead",
  "RunsPause",
  "RunsResume",
  "RunsStop",
  "PerceptsAppend",
  "ActionsSubmit",
  "TracesRead",
  "StateRead",
  "ReplayCreate",
  "MessagesSend",
  "HealthRead",
  "CapabilitiesRead",
  "NodesRegister",
  "InstancesRegister",
  "NodesHeartbeat",
  "InstancesHeartbeat"
] as const;

export const ENDPOINT_SCOPE_LABELS: Record<EndpointScope, string> = {
  runs_create: "splendor.runs.create",
  runs_start: "splendor.runs.start",
  runs_read: "splendor.runs.read",
  runs_pause: "splendor.runs.pause",
  runs_resume: "splendor.runs.resume",
  runs_stop: "splendor.runs.stop",
  percepts_append: "splendor.percepts.append",
  actions_submit: "splendor.actions.submit",
  traces_read: "splendor.traces.read",
  state_read: "splendor.state.read",
  replay_create: "splendor.replay.create",
  messages_send: "splendor.messages.send",
  health_read: "splendor.health.read",
  capabilities_read: "splendor.capabilities.read",
  nodes_register: "splendor.nodes.register",
  instances_register: "splendor.instances.register",
  nodes_heartbeat: "splendor.nodes.heartbeat",
  instances_heartbeat: "splendor.instances.heartbeat"
};

export const DAEMON_API_COMPATIBILITY = {
  milestone: "0.02-dev",
  sprint: "0.02-S6",
  daemonApiVersion: "0.02-dev",
  schemaSource: "splendor-types Rust crates and docs/reference"
} as const;
