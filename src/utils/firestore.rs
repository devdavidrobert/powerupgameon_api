use crate::error::{ApiError, ApiResult};
use firestore::{FirestoreQueryCursor, FirestoreValue};
use gcloud_sdk::google::firestore::v1::value;
use serde_json::{json, Map, Value};

pub fn serialize_doc_data(data: &Map<String, Value>) -> Map<String, Value> {
    let mut out = data.clone();
    for (key, value) in out.clone().iter() {
        if let Some(iso) = value_to_iso(value) {
            out.insert(key.clone(), Value::String(iso));
        }
    }
    out
}

pub fn value_to_iso(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) if n.as_i64().is_some() => {
            let ms = n.as_i64().unwrap();
            // Only epoch-millis timestamps (not small counters like score/percentage).
            if ms >= 946_684_800_000 {
                chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.to_rfc3339())
            } else {
                None
            }
        }
        Value::Object(obj) if obj.contains_key("_seconds") => {
            let secs = obj.get("_seconds")?.as_i64()?;
            let nanos = obj.get("_nanoseconds").and_then(|n| n.as_i64()).unwrap_or(0);
            chrono::DateTime::from_timestamp(secs, (nanos * 1000) as u32)
                .map(|dt| dt.to_rfc3339())
        }
        _ => None,
    }
}

pub fn millis_now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn millis_from_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.timestamp_millis()),
        Value::Object(obj) if obj.contains_key("_seconds") => {
            let secs = obj.get("_seconds")?.as_i64()?;
            let nanos = obj.get("_nanoseconds").and_then(|n| n.as_i64()).unwrap_or(0);
            Some(secs * 1000 + nanos / 1_000_000)
        }
        _ => None,
    }
}

/// Reads a Firestore document id from a deserialized row.
/// firestore 0.47 injects `_firestore_id`; older code paths used `__name__` or an explicit `id` field.
pub fn document_id_from_map(row: &Map<String, Value>) -> Option<String> {
    row.get("_firestore_id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            row.get("__name__")
                .and_then(|v| v.as_str())
                .map(|s| s.rsplit('/').next().unwrap_or(s).to_string())
        })
        .or_else(|| row.get("id").and_then(|v| v.as_str()).map(String::from))
}

/// Full Firestore document reference path for a subcollection document.
pub fn document_ref_path(parent: &str, subcol: &str, doc_id: &str) -> String {
    format!("{parent}/{subcol}/{doc_id}")
}

/// Resolve the full document reference path from a query row.
pub fn document_ref_from_row(
    row: &Map<String, Value>,
    parent: &str,
    subcol: &str,
) -> Option<String> {
    row.get("__name__")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            document_id_from_map(row).map(|id| document_ref_path(parent, subcol, &id))
        })
}

fn firestore_integer(v: i64) -> FirestoreValue {
    FirestoreValue::from(gcloud_sdk::google::firestore::v1::Value {
        value_type: Some(value::ValueType::IntegerValue(v)),
    })
}

fn firestore_reference(path: &str) -> FirestoreValue {
    FirestoreValue::from(gcloud_sdk::google::firestore::v1::Value {
        value_type: Some(value::ValueType::ReferenceValue(path.to_string())),
    })
}

/// Cursor field values matching composite `(timestamp DESC, __name__ DESC)` ordering.
pub fn pagination_cursor_values(timestamp: i64, doc_ref_path: &str) -> Vec<FirestoreValue> {
    vec![
        firestore_integer(timestamp),
        firestore_reference(doc_ref_path),
    ]
}

pub fn parse_page_cursor(
    cursor: &Map<String, Value>,
    timestamp_field: &str,
) -> Option<(i64, String)> {
    let ts = cursor.get(timestamp_field).and_then(millis_from_value)?;
    let name = cursor.get("name").and_then(|v| v.as_str())?.to_string();
    Some((ts, name))
}

pub fn build_page_cursor(
    row: &Map<String, Value>,
    timestamp_field: &str,
    parent: &str,
    subcol: &str,
) -> Option<Map<String, Value>> {
    let ts = row.get(timestamp_field).and_then(millis_from_value)?;
    let name = document_ref_from_row(row, parent, subcol)?;
    let mut cursor = Map::new();
    cursor.insert(timestamp_field.into(), json!(ts));
    cursor.insert("name".into(), json!(name));
    Some(cursor)
}

pub fn start_after_cursor(
    cursor: &Map<String, Value>,
    timestamp_field: &str,
) -> ApiResult<FirestoreQueryCursor> {
    let (ts, name) = parse_page_cursor(cursor, timestamp_field).ok_or_else(|| {
        ApiError::bad_request("Invalid pagination cursor.")
    })?;
    Ok(FirestoreQueryCursor::AfterValue(pagination_cursor_values(
        ts, &name,
    )))
}
