use crate::format::format_value;
use prism_analysis::{filtered_total, top, Filter, TopSort};
use prism_store::Store;

pub struct Args {
    pub reference: String,
    pub top: usize,
    pub st: Option<String>,
    pub focus: Option<String>,
    pub exclude: Option<String>,
    pub hide: Option<String>,
    pub sort: TopSort,
}

pub fn run(store: &Store, a: Args) -> Result<(), String> {
    let rec = store.resolve(&a.reference).map_err(|e| e.to_string())?;
    let profile = store.load(&rec.id).map_err(|e| e.to_string())?;
    if profile.sample_types.is_empty() {
        return Err("profile has no sample types".into());
    }
    let st = match &a.st {
        None => profile.default_sample_type,
        Some(n) => profile.sample_type_index(n).ok_or_else(|| {
            let avail: Vec<&str> = profile
                .sample_types
                .iter()
                .map(|t| t.name.as_str())
                .collect();
            format!(
                "unknown sample type '{n}' (available: {})",
                avail.join(", ")
            )
        })?,
    };
    if st >= profile.sample_types.len() {
        return Err(format!("sample type index {st} out of range"));
    }
    let unit = rec
        .sample_types
        .get(st)
        .map(|s| s.unit.clone())
        .unwrap_or_else(|| "count".into());
    let filter = Filter::parse(a.focus.as_deref(), a.exclude.as_deref(), a.hide.as_deref())
        .map_err(|e| format!("invalid regex: {e}"))?;
    let rows = top(&profile, st, &filter, a.top, a.sort);
    let n = profile.sample_types.len();
    let unfiltered = profile
        .values
        .chunks(n)
        .fold(0i64, |acc, c| acc.saturating_add(c[st]));
    let total = if filter.is_empty() {
        format_value(unfiltered as f64, &unit)
    } else {
        format!(
            "{} (of {})",
            format_value(filtered_total(&profile, st, &filter) as f64, &unit),
            format_value(unfiltered as f64, &unit)
        )
    };

    println!("{}", rec.name);
    println!(
        "kind: {}  fidelity: {}  sample type: {}  total: {}",
        super::kind_str(&rec.kind),
        super::kind_str(&rec.fidelity),
        profile.sample_types[st].name,
        total
    );
    println!();
    let mut table = vec![[
        "SELF".to_string(),
        "SELF%".into(),
        "TOTAL".into(),
        "TOTAL%".into(),
        "FUNCTION".into(),
    ]];
    for r in &rows {
        table.push([
            format_value(r.self_value as f64, &unit),
            format!("{:.2}%", r.self_pct),
            format_value(r.total as f64, &unit),
            format!("{:.2}%", r.total_pct),
            r.name.clone(),
        ]);
    }
    let mut w = [0usize; 4];
    for row in &table {
        for (i, width) in w.iter_mut().enumerate() {
            *width = (*width).max(row[i].chars().count());
        }
    }
    for row in &table {
        let mut line = String::new();
        for (i, width) in w.iter().enumerate() {
            line.push_str(&" ".repeat(width - row[i].chars().count()));
            line.push_str(&row[i]);
            line.push_str("  ");
        }
        line.push_str(&row[4]);
        println!("{line}");
    }
    Ok(())
}
