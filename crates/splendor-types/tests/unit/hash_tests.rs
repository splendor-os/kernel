use super::*;

#[test]
fn blake3_hash_is_deterministic() {
    let first = ContentHash::blake3(b"kernel");
    let second = ContentHash::blake3(b"kernel");
    assert_eq!(first, second);
    assert!(first.value.starts_with('a') || !first.value.is_empty());
}

#[test]
fn hash_display_includes_algorithm() {
    let hash = ContentHash::blake3(b"state");
    assert!(hash.to_string().starts_with("blake3:"));
}

#[test]
fn parse_algorithm_round_trip() {
    let algorithm = HashAlgorithm::parse("blake3").expect("parse");
    assert_eq!(algorithm.as_str(), "blake3");
    assert!(HashAlgorithm::parse("unknown").is_none());
}
