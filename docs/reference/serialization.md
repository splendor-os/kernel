# Serialization and Determinism

This document defines the serialization and hashing rules required for
reproducible kernel runs.

## Serialization Rules

- All core objects derive `serde::Serialize` and `serde::Deserialize`.
- JSON and CBOR round-trips must preserve semantic equality.
- `TraceEvent.sequence` is strictly monotonic within a `RunId`.

## Timestamp Handling

- `TraceEvent.timestamp` is captured at emission time and persisted verbatim.
- Replay and audit tooling must use stored timestamps instead of wall-clock time.

## Content Hashing

`ContentHash` uses `blake3` for deterministic digests. The canonical string
format is:

```
{algorithm}:{hex_digest}
```

## Snapshot IDs

`SnapshotId` is derived directly from the snapshot bytes:

```
SnapshotId = ContentHash::blake3(state_bytes)
```

## State Node Hashing

`StateNodeId` is derived from a serialized `StateNodeHashInput` payload:

```
{
  parent_ids: [StateNodeId],
  data_hash: ContentHash,
  metadata: StateMetadata
}
```

The payload is serialized with `serde_json` and hashed with BLAKE3. Any change
in ordering or field representation will produce a new node ID.

## Trace Integrity Hashing

Trace records use a hash chain to support integrity checks:

```
event_hash = blake3(prev_hash_string || payload_bytes)
```

`prev_hash_string` is the `ContentHash` string form (`algorithm:value`) of the
previous event. The first record in a run uses only payload bytes.
