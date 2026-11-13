//! Rust port of the UI `formatValue` rules (ui/src/format.ts).

fn to_sig3(v: f64) -> f64 {
    format!("{v:.2e}").parse().unwrap_or(v)
}

/// 3 significant digits, trailing zeros trimmed.
fn sig3(v: f64) -> String {
    format!("{}", to_sig3(v))
}

fn scaled(v: f64, step: f64, units: &[&str]) -> String {
    let mut i = 0;
    let mut x = v.abs();
    while i < units.len() - 1 && to_sig3(x) >= step {
        x /= step;
        i += 1;
    }
    format!("{}{} {}", if v < 0.0 { "-" } else { "" }, sig3(x), units[i])
}

fn round(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// `toLocaleString('en-US')`: thousands separators, up to 3 fraction digits.
fn group(v: f64) -> String {
    let s = format!("{:.3}", v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let frac = frac.trim_end_matches('0');
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if !frac.is_empty() {
        out.push('.');
        out.push_str(frac);
    }
    if v < 0.0 && out != "0" {
        out.insert(0, '-');
    }
    out
}

pub fn format_value(v: f64, unit: &str) -> String {
    match unit {
        "nanoseconds" => {
            let a = v.abs();
            if a < 1000.0 {
                format!("{} ns", round(v))
            } else if a < 1e6 {
                scaled(v / 1e3, 1000.0, &["µs", "ms", "s"])
            } else if a < 1e9 {
                scaled(v / 1e6, 1000.0, &["ms", "s"])
            } else {
                format!("{} s", sig3(v / 1e9))
            }
        }
        "bytes" => {
            if v.abs() < 1024.0 {
                format!("{} B", round(v))
            } else {
                scaled(v / 1024.0, 1024.0, &["KiB", "MiB", "GiB", "TiB"])
            }
        }
        "count" => group(v),
        other => format!("{} {}", group(v), other),
    }
}

#[cfg(test)]
mod tests {
    use super::format_value as f;

    #[test]
    fn nanoseconds_three_sig_digits() {
        assert_eq!(f(1_234_000_000.0, "nanoseconds"), "1.23 s");
        assert_eq!(f(45_600_000.0, "nanoseconds"), "45.6 ms");
        assert_eq!(f(789_000.0, "nanoseconds"), "789 µs");
        assert_eq!(f(12.0, "nanoseconds"), "12 ns");
        assert_eq!(f(999_999.0, "nanoseconds"), "1 ms");
    }

    #[test]
    fn bytes_binary_units() {
        assert_eq!(f(1536.0, "bytes"), "1.5 KiB");
        assert_eq!(f(512.0 * 1024.0, "bytes"), "512 KiB");
        assert_eq!(f(10.0, "bytes"), "10 B");
        assert_eq!(f(1.5 * 1024f64.powi(3), "bytes"), "1.5 GiB");
    }

    #[test]
    fn counts_with_separators() {
        assert_eq!(f(12345.0, "count"), "12,345");
        assert_eq!(f(0.0, "count"), "0");
        assert_eq!(f(1_234_567.0, "count"), "1,234,567");
    }

    #[test]
    fn other_units_fall_back() {
        assert_eq!(f(7.0, "widgets"), "7 widgets");
    }
}
