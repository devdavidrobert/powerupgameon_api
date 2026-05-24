use serde_json::{Map, Value};

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

pub fn doc_to_map<T: serde::Serialize>(value: &T) -> Map<String, Value> {
    match serde_json::to_value(value).unwrap_or(Value::Null) {
        Value::Object(map) => map,
        _ => Map::new(),
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
