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
    assert_eq!(
        HashAlgorithm::parse("sha256").expect("parse").as_str(),
        "sha256"
    );
    assert!(HashAlgorithm::parse("unknown").is_none());
}

#[test]
fn content_hash_parse_round_trip() {
    let blake3 = ContentHash::blake3(b"state");
    assert_eq!(ContentHash::parse(&blake3.to_string()), Some(blake3));

    let sha256 = ContentHash::new(HashAlgorithm::Sha256, "abc123");
    assert_eq!(ContentHash::parse("sha256:abc123"), Some(sha256));
    assert!(ContentHash::parse("sha256:").is_none());
    assert!(ContentHash::parse("unknown:abc123").is_none());
}
