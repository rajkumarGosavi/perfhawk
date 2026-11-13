pub mod diff;
pub mod import;
pub mod ls;
pub mod record;
pub mod rm;
pub mod serve;
pub mod sessions;
pub mod show;
pub mod targets;

/// snake_case serde string of a unit-variant enum (e.g. `block_io`).
pub fn kind_str<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Prints `rows` (first row is the header) as a padded table. Columns listed in
/// `right` are right-aligned; the last column is never padded.
pub fn print_table(rows: &[Vec<String>], right: &[usize]) {
    let cols = rows.first().map_or(0, |r| r.len());
    let mut widths = vec![0usize; cols];
    for row in rows {
        for (w, c) in widths.iter_mut().zip(row) {
            *w = (*w).max(c.chars().count());
        }
    }
    for row in rows {
        let mut line = String::new();
        for (i, c) in row.iter().enumerate() {
            if i + 1 == cols {
                line.push_str(c);
                continue;
            }
            let pad = " ".repeat(widths[i] - c.chars().count());
            if right.contains(&i) {
                line.push_str(&pad);
                line.push_str(c);
            } else {
                line.push_str(c);
                line.push_str(&pad);
            }
            line.push_str("  ");
        }
        println!("{}", line.trim_end());
    }
}

/// `30.2s` from milliseconds.
pub fn format_duration_ms(ms: i64) -> String {
    format!("{:.1}s", ms as f64 / 1000.0)
}
