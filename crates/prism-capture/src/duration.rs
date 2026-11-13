use std::time::Duration;

pub const MIN_DURATION: Duration = Duration::from_secs(1);
pub const MAX_DURATION: Duration = Duration::from_secs(600);
pub const DEFAULT_DURATION: Duration = Duration::from_secs(30);

/// Parses `<n>ms`, `<n>s` or `<n>m` and enforces the 1 s .. 10 min range.
pub fn parse_duration(s: &str) -> Result<Duration, String> {
    let bad = || format!("invalid duration '{s}': use <n>ms, <n>s or <n>m (e.g. 500ms, 30s, 2m)");
    let split = s.find(|c: char| !c.is_ascii_digit()).ok_or_else(bad)?;
    let (num, unit) = s.split_at(split);
    if num.is_empty() {
        return Err(bad());
    }
    let n: u64 = num.parse().map_err(|_| bad())?;
    let d = match unit {
        "ms" => Duration::from_millis(n),
        "s" => Duration::from_secs(n),
        "m" => Duration::from_secs(n.checked_mul(60).ok_or_else(bad)?),
        _ => return Err(bad()),
    };
    if d < MIN_DURATION {
        return Err(format!("duration '{s}' is below the 1s minimum"));
    }
    if d > MAX_DURATION {
        return Err(format!("duration '{s}' is above the 10m maximum"));
    }
    Ok(d)
}
