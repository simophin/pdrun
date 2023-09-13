use chrono_tz::Tz;

pub fn current_timezone() -> Tz {
    "Australia/Melbourne".parse().unwrap_or_else(|_e| Tz::UTC)
}
