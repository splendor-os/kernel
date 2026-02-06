use super::*;

trait AsUuid {
    fn as_uuid(&self) -> &Uuid;
}

impl AsUuid for TenantId {
    fn as_uuid(&self) -> &Uuid {
        TenantId::as_uuid(self)
    }
}

impl AsUuid for AgentId {
    fn as_uuid(&self) -> &Uuid {
        AgentId::as_uuid(self)
    }
}

impl AsUuid for RunId {
    fn as_uuid(&self) -> &Uuid {
        RunId::as_uuid(self)
    }
}

impl AsUuid for TraceId {
    fn as_uuid(&self) -> &Uuid {
        TraceId::as_uuid(self)
    }
}

fn assert_uuid<T>(id: T)
where
    T: Clone + Eq + From<Uuid> + std::fmt::Display + AsUuid + std::fmt::Debug,
{
    let uuid = *id.as_uuid();
    let round_trip = T::from(uuid);
    assert_eq!(id, round_trip);
    assert_eq!(uuid.to_string(), id.to_string());
}

#[test]
fn id_round_trips() {
    assert_uuid(TenantId::new());
    assert_uuid(TenantId::default());
    assert_uuid(AgentId::new());
    assert_uuid(AgentId::default());
    assert_uuid(RunId::new());
    assert_uuid(RunId::default());
    assert_uuid(TraceId::new());
    assert_uuid(TraceId::default());
}

#[test]
fn trace_id_deterministic_from_run_sequence() {
    let run_id = RunId::new();
    let first = TraceId::from_run_sequence(&run_id, 7);
    let second = TraceId::from_run_sequence(&run_id, 7);
    assert_eq!(first, second);
}

#[test]
fn snapshot_id_from_bytes_is_stable() {
    let snapshot = SnapshotId::from_bytes(b"state");
    assert_eq!(snapshot.to_string(), snapshot.hash().to_string());

    let from_hash = SnapshotId::from_hash(ContentHash::blake3(b"state"));
    assert_eq!(from_hash, snapshot);
}
