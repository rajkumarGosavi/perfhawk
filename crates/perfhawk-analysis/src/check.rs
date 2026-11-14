//! The regression rule behind `perfhawk check` and `GET /api/check`.

use crate::{diff_top_sorted, match_sample_type, DiffInput, DiffNormalize, DiffSort, Filter};
use perfhawk_model::Profile;
use serde::Serialize;

pub struct CheckInput<'a> {
    pub base: &'a Profile,
    pub head: &'a Profile,
    pub base_name: &'a str,
    pub head_name: &'a str,
    pub sample_type: Option<&'a str>,
    /// Growth in percent above which a function regresses.
    pub threshold: f64,
    /// Minimum growth of a function's share, in percentage points.
    pub min_value: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Regression {
    pub name: String,
    pub base_self: i64,
    pub head_self: i64,
    /// `None` for a function that is new in the head.
    pub growth_pct: Option<f64>,
    /// Change in the function's share of its own profile, in percentage points.
    pub delta_points: f64,
}

#[derive(Debug, Clone)]
pub struct CheckReport {
    pub ok: bool,
    /// Index of the compared sample type in the base and head profiles.
    pub base_index: usize,
    pub head_index: usize,
    pub sample_type_name: String,
    pub base_total: i64,
    pub head_total: i64,
    pub regressions: Vec<Regression>,
}

/// Compares `head` with `base`. Errors are user-facing data errors.
pub fn run_check(i: &CheckInput) -> Result<CheckReport, String> {
    let (base, head) = (i.base, i.head);
    if let Some(n) = i.sample_type {
        if base.sample_type_index(n).is_none() || head.sample_type_index(n).is_none() {
            return Err(format!("sample type '{n}' is not present in both profiles"));
        }
    }
    let (bi, hi) = match_sample_type(base, head, i.sample_type).map_err(|e| e.to_string())?;
    let (bt, ht) = (base.total(bi), head.total(hi));
    if bt <= 0 {
        return Err(format!(
            "the baseline '{}' has no samples to compare against",
            i.base_name
        ));
    }
    if ht <= 0 {
        return Err(format!("the profile '{}' has no samples", i.head_name));
    }
    let rows = diff_top_sorted(
        DiffInput {
            profile: base,
            sample_type: bi,
        },
        DiffInput {
            profile: head,
            sample_type: hi,
        },
        &Filter::default(),
        DiffNormalize::None,
        usize::MAX,
        DiffSort::Delta,
    );
    let pct = |v: i64, total: i64| v as f64 / total as f64 * 100.0;
    let mut regressions: Vec<Regression> = rows
        .into_iter()
        .filter_map(|r| {
            let delta_points = pct(r.head_self, ht) - pct(r.base_self, bt);
            let growth = growth_pct(
                r.base_self,
                r.head_self,
                base.meta.duration_ns,
                head.meta.duration_ns,
            );
            let regressed = match growth {
                Some(g) => g > i.threshold && delta_points >= i.min_value,
                None => r.head_self > 0 && pct(r.head_self, ht) >= i.min_value,
            };
            regressed.then_some(Regression {
                name: r.name,
                base_self: r.base_self,
                head_self: r.head_self,
                growth_pct: growth,
                delta_points,
            })
        })
        .collect();
    regressions.sort_by(|x, y| {
        y.delta_points
            .total_cmp(&x.delta_points)
            .then_with(|| x.name.cmp(&y.name))
    });
    Ok(CheckReport {
        ok: regressions.is_empty(),
        base_index: bi,
        head_index: hi,
        sample_type_name: base.sample_types[bi].name.clone(),
        base_total: bt,
        head_total: ht,
        regressions,
    })
}

/// Growth of a function's own cost in percent, or `None` when it is new in the head.
/// When both profiles record their duration the baseline is scaled to the head's run
/// length, so a shorter or longer run does not hide or invent a regression; without
/// durations the raw values are compared.
pub fn growth_pct(
    base_self: i64,
    head_self: i64,
    base_dur: Option<u64>,
    head_dur: Option<u64>,
) -> Option<f64> {
    if base_self <= 0 {
        return None;
    }
    let scale = match (base_dur, head_dur) {
        (Some(b), Some(h)) if b > 0 && h > 0 => h as f64 / b as f64,
        _ => 1.0,
    };
    Some((head_self as f64 / (base_self as f64 * scale) - 1.0) * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfhawk_model::{
        Fidelity, Profile, ProfileBuilder, ProfileKind, ProfileMeta, SampleType, SourceFormat, Unit,
    };

    fn profile(rows: &[(&str, i64)], duration_ns: Option<u64>) -> Profile {
        let mut b = ProfileBuilder::new(
            vec![SampleType {
                name: "time".into(),
                unit: Unit::Nanoseconds,
            }],
            ProfileMeta {
                kind: ProfileKind::Cpu,
                source_format: SourceFormat::Pprof,
                fidelity: Fidelity::Native,
                duration_ns,
                captured_at_unix_ns: None,
                period: None,
            },
        );
        for (name, v) in rows {
            let f = b.function(name, "", 0);
            let fr = b.frame(f, 0, 0);
            let st = b.stack(&[fr]);
            b.add_sample(st, &[*v], vec![]).unwrap();
        }
        b.finish()
    }

    fn input<'a>(base: &'a Profile, head: &'a Profile) -> CheckInput<'a> {
        CheckInput {
            base,
            head,
            base_name: "b",
            head_name: "h",
            sample_type: None,
            threshold: 10.0,
            min_value: 1.0,
        }
    }

    #[test]
    fn raw_growth_without_durations() {
        assert_eq!(growth_pct(100, 300, None, None), Some(200.0));
        assert_eq!(growth_pct(0, 5, None, None), None);
        assert_eq!(growth_pct(100, 100, Some(5), None), Some(0.0));
    }

    #[test]
    fn a_shorter_head_run_does_not_hide_a_regression() {
        // Half as long a run and the function still cost 75 (was 100): 50% worse per second.
        let g = growth_pct(100, 75, Some(10_000), Some(5_000)).unwrap();
        assert!((g - 50.0).abs() < 1e-9, "{g}");
    }

    #[test]
    fn a_longer_head_run_does_not_invent_one() {
        let g = growth_pct(100, 150, Some(10_000), Some(15_000)).unwrap();
        assert!(g.abs() < 1e-9, "{g}");
    }

    #[test]
    fn run_check_flags_new_function() {
        let base = profile(&[("a", 100)], None);
        let head = profile(&[("a", 100), ("fresh", 50)], None);
        let r = run_check(&input(&base, &head)).unwrap();
        assert!(!r.ok);
        let new = r.regressions.iter().find(|x| x.name == "fresh").unwrap();
        assert_eq!(new.growth_pct, None);
    }

    #[test]
    fn run_check_ok_when_identical() {
        let base = profile(&[("a", 100), ("b", 50)], None);
        let head = profile(&[("a", 100), ("b", 50)], None);
        let r = run_check(&input(&base, &head)).unwrap();
        assert!(r.ok);
        assert!(r.regressions.is_empty());
        assert_eq!((r.base_total, r.head_total), (150, 150));
    }

    #[test]
    fn run_check_empty_baseline_errors() {
        let base = profile(&[], None);
        let head = profile(&[("a", 1)], None);
        let e = run_check(&input(&base, &head)).unwrap_err();
        assert_eq!(e, "the baseline 'b' has no samples to compare against");
    }

    #[test]
    fn run_check_unknown_sample_type_errors() {
        let base = profile(&[("a", 1)], None);
        let head = profile(&[("a", 1)], None);
        let mut i = input(&base, &head);
        i.sample_type = Some("nope");
        assert_eq!(
            run_check(&i).unwrap_err(),
            "sample type 'nope' is not present in both profiles"
        );
    }
}
