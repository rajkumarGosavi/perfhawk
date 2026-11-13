use super::print_table;
use crate::format::format_value;
use prism_analysis::{diff_top, match_sample_type, DiffInput, DiffNormalize, Filter};
use prism_store::Store;

pub struct Args {
    pub base: String,
    pub head: String,
    pub st: Option<String>,
    pub normalize: DiffNormalize,
    pub top: usize,
    pub focus: Option<String>,
    pub exclude: Option<String>,
    pub hide: Option<String>,
}

fn signed(v: i64, unit: &str) -> String {
    let s = format_value(v as f64, unit);
    if v > 0 {
        format!("+{s}")
    } else {
        s
    }
}

pub fn run(store: &Store, a: Args) -> Result<(), String> {
    let brec = store.resolve(&a.base).map_err(|e| e.to_string())?;
    let hrec = store.resolve(&a.head).map_err(|e| e.to_string())?;
    let base = store.load(&brec.id).map_err(|e| e.to_string())?;
    let head = store.load(&hrec.id).map_err(|e| e.to_string())?;
    if let Some(n) = &a.st {
        if base.sample_type_index(n).is_none() || head.sample_type_index(n).is_none() {
            return Err(format!("sample type '{n}' is not present in both profiles"));
        }
    }
    let (bi, hi) = match_sample_type(&base, &head, a.st.as_deref()).map_err(|e| e.to_string())?;
    let filter = Filter::parse(a.focus.as_deref(), a.exclude.as_deref(), a.hide.as_deref())
        .map_err(|e| format!("invalid regex: {e}"))?;
    let unit = brec
        .sample_types
        .get(bi)
        .map(|s| s.unit.clone())
        .unwrap_or_else(|| "count".into());
    let rows = diff_top(
        DiffInput {
            profile: &base,
            sample_type: bi,
        },
        DiffInput {
            profile: &head,
            sample_type: hi,
        },
        &filter,
        a.normalize,
        a.top,
    );
    println!(
        "base: {} ({})  head: {} ({})  sample type: {}",
        brec.name,
        format_value(base.total(bi) as f64, &unit),
        hrec.name,
        format_value(head.total(hi) as f64, &unit),
        base.sample_types[bi].name
    );
    println!();
    let mut table = vec![[
        "DELTA SELF",
        "BASE SELF",
        "HEAD SELF",
        "DELTA TOTAL",
        "FUNCTION",
    ]
    .map(String::from)
    .to_vec()];
    for r in rows {
        table.push(vec![
            signed(r.delta_self, &unit),
            format_value(r.base_self as f64, &unit),
            format_value(r.head_self as f64, &unit),
            signed(r.delta_total, &unit),
            r.name,
        ]);
    }
    print_table(&table, &[0, 1, 2, 3]);
    Ok(())
}
