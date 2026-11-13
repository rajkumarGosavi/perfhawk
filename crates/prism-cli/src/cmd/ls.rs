use crate::format::format_value;
use prism_store::Store;

pub fn run(store: &Store, json: bool) -> Result<(), String> {
    let recs = store.list().map_err(|e| e.to_string())?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&recs).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    let mut rows: Vec<[String; 7]> = vec![[
        "ID", "KIND", "FORMAT", "SAMPLES", "TOTAL", "NAME", "IMPORTED",
    ]
    .map(String::from)];
    for r in &recs {
        let unit = r
            .sample_types
            .get(r.default_sample_type)
            .map(|s| s.unit.as_str())
            .unwrap_or("count");
        rows.push([
            r.short_id.clone(),
            super::kind_str(&r.kind),
            super::kind_str(&r.source_format),
            r.sample_count.to_string(),
            format_value(r.default_total as f64, unit),
            r.name.clone(),
            imported(r.imported_at_unix_ms),
        ]);
    }
    let mut widths = [0usize; 7];
    for row in &rows {
        for (w, c) in widths.iter_mut().zip(row) {
            *w = (*w).max(c.chars().count());
        }
    }
    for row in &rows {
        let mut line = String::new();
        for (i, c) in row.iter().enumerate() {
            if i == 6 {
                line.push_str(c);
                continue;
            }
            let pad = " ".repeat(widths[i] - c.chars().count());
            if i == 3 || i == 4 {
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
    Ok(())
}

/// UTC `YYYY-MM-DD HH:MM` from unix milliseconds.
fn imported(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn imported_formats_utc() {
        assert_eq!(super::imported(0), "1970-01-01 00:00");
        assert_eq!(super::imported(1_700_000_000_000), "2023-11-14 22:13");
    }
}
