use serde_json::Value;

// A dangling component reference breaks generated clients even when HTTP handlers work.
#[test]
fn generated_contract_has_no_dangling_local_references() {
    let document = serde_json::to_value(learner_progress_service::routes::openapi()).unwrap();
    fn check(value: &Value, document: &Value) {
        match value {
            Value::Object(fields) => {
                if let Some(Value::String(reference)) = fields.get("$ref") {
                    let pointer = reference.strip_prefix('#').expect("local schema reference");
                    assert!(
                        document.pointer(pointer).is_some(),
                        "unresolved reference: {reference}"
                    );
                }
                for field in fields.values() {
                    check(field, document);
                }
            }
            Value::Array(values) => {
                for value in values {
                    check(value, document);
                }
            }
            _ => {}
        }
    }
    check(&document, &document);
}
