use crate::config::Config;
use serde_json::json;

pub fn log(config: &Config, level: &str, message: &str, meta: serde_json::Value) {
    let mut line = json!({
        "level": level,
        "message": message,
        "time": chrono::Utc::now().to_rfc3339(),
    });
    if let Some(obj) = line.as_object_mut() {
        if let Some(meta_obj) = meta.as_object() {
            for (k, v) in meta_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
    let text = line.to_string();
    match level {
        "error" => tracing::error!("{text}"),
        "warn" => tracing::warn!("{text}"),
        _ if config.is_production && level == "info" => {}
        _ => tracing::info!("{text}"),
    }
}
