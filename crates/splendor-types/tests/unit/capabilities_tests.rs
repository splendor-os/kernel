use super::*;

#[test]
fn valid_capability_document_round_trips() {
    let document = CapabilityDocument::new(
        vec![
            "runtime.resident".to_string(),
            "http.egress.restricted".to_string(),
            "camera.rgb".to_string(),
        ],
        serde_json::json!({"max_http_requests_per_minute": 60}),
    )
    .expect("valid capability document");

    document.validate().expect("still valid");
    assert_eq!(document.schema, CAPABILITY_DOCUMENT_SCHEMA);

    let payload = serde_json::to_vec(&document).expect("serialize");
    let decoded: CapabilityDocument = serde_json::from_slice(&payload).expect("deserialize");
    assert_eq!(decoded, document);
}

#[test]
fn missing_constraints_default_to_empty_object() {
    let decoded: CapabilityDocument = serde_json::from_value(serde_json::json!({
        "schema": CAPABILITY_DOCUMENT_SCHEMA,
        "capabilities": ["runtime.resident"]
    }))
    .expect("deserialize with default constraints");

    assert_eq!(decoded.constraints, serde_json::json!({}));
    decoded.validate().expect("defaulted constraints valid");
}

#[test]
fn invalid_capability_documents_fail_closed() {
    let missing_schema = CapabilityDocument {
        schema: " ".to_string(),
        capabilities: vec!["runtime.resident".to_string()],
        constraints: serde_json::json!({}),
    };
    assert_eq!(
        missing_schema.validate(),
        Err(CapabilityValidationError::MissingSchema)
    );

    let unsupported_schema = CapabilityDocument {
        schema: "splendor.capabilities.v2".to_string(),
        capabilities: vec!["runtime.resident".to_string()],
        constraints: serde_json::json!({}),
    };
    assert_eq!(
        unsupported_schema.validate(),
        Err(CapabilityValidationError::UnsupportedSchema {
            schema: "splendor.capabilities.v2".to_string()
        })
    );

    let empty_capabilities = CapabilityDocument {
        schema: CAPABILITY_DOCUMENT_SCHEMA.to_string(),
        capabilities: vec![],
        constraints: serde_json::json!({}),
    };
    assert_eq!(
        empty_capabilities.validate(),
        Err(CapabilityValidationError::EmptyCapabilities)
    );

    let invalid_name = CapabilityDocument {
        schema: CAPABILITY_DOCUMENT_SCHEMA.to_string(),
        capabilities: vec!["http egress".to_string()],
        constraints: serde_json::json!({}),
    };
    assert_eq!(
        invalid_name.validate(),
        Err(CapabilityValidationError::InvalidCapabilityName {
            name: "http egress".to_string()
        })
    );

    let duplicate = CapabilityDocument {
        schema: CAPABILITY_DOCUMENT_SCHEMA.to_string(),
        capabilities: vec![
            "runtime.resident".to_string(),
            "runtime.resident".to_string(),
        ],
        constraints: serde_json::json!({}),
    };
    assert_eq!(
        duplicate.validate(),
        Err(CapabilityValidationError::DuplicateCapability {
            name: "runtime.resident".to_string()
        })
    );

    let invalid_constraints = CapabilityDocument {
        schema: CAPABILITY_DOCUMENT_SCHEMA.to_string(),
        capabilities: vec!["runtime.resident".to_string()],
        constraints: serde_json::json!(["not", "object"]),
    };
    assert_eq!(
        invalid_constraints.validate(),
        Err(CapabilityValidationError::InvalidConstraintsDocument)
    );
}

#[test]
fn capability_name_validation_rejects_ambiguous_tokens() {
    for valid in [
        "runtime.resident",
        "http.egress-restricted",
        "local_llm.small",
    ] {
        assert!(is_valid_capability_name(valid), "{valid} should be valid");
    }

    for invalid in [
        "",
        " runtime.resident",
        ".runtime",
        "runtime.",
        "runtime..gpu",
        "http egress",
    ] {
        assert!(
            !is_valid_capability_name(invalid),
            "{invalid:?} should be invalid"
        );
    }
}
