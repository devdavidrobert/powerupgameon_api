pub const DEFAULT_IP_RATE_LIMIT_WINDOW_SECS: u64 = 60 * 60;
pub const MIN_IP_RATE_LIMIT_WINDOW_SECS: i64 = 60;
pub const MAX_IP_RATE_LIMIT_WINDOW_SECS: i64 = 7 * 24 * 60 * 60;

pub fn normalize_ip_rate_limit_window_secs(value: i64) -> i64 {
    value.clamp(MIN_IP_RATE_LIMIT_WINDOW_SECS, MAX_IP_RATE_LIMIT_WINDOW_SECS)
}

pub fn resolve_ip_rate_limit_window_secs(stored: Option<i64>) -> u64 {
    stored
        .map(normalize_ip_rate_limit_window_secs)
        .unwrap_or(DEFAULT_IP_RATE_LIMIT_WINDOW_SECS as i64) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_one_hour_when_unset() {
        assert_eq!(resolve_ip_rate_limit_window_secs(None), 3600);
    }

    #[test]
    fn clamps_out_of_range_values() {
        assert_eq!(normalize_ip_rate_limit_window_secs(10), MIN_IP_RATE_LIMIT_WINDOW_SECS);
        assert_eq!(
            normalize_ip_rate_limit_window_secs(999_999),
            MAX_IP_RATE_LIMIT_WINDOW_SECS
        );
    }
}
