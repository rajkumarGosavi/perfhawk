//! Turns per-sample `prism.bucket` labels (written by the Go agent and the
//! .NET helper) into a [`Timeline`] sidecar.

use prism_model::{LabelValue, Profile, Timeline};

/// The label key runtimes use to say which time bucket a sample belongs to.
pub const BUCKET_LABEL: &str = "prism.bucket";

/// One bucket per sample, in sample order, or `None` when any sample lacks a
/// usable `prism.bucket` label (the profile then simply has no time data).
pub fn timeline_from_labels(p: &Profile, bucket_ns: u64, start_unix_ns: i64) -> Option<Timeline> {
    if bucket_ns == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(p.samples.len());
    for s in &p.samples {
        let label = s.labels.iter().find(|l| p.str(l.key) == BUCKET_LABEL)?;
        let b = match &label.value {
            LabelValue::Str(id) => p.str(*id).parse::<u32>().ok()?,
            LabelValue::Num { value, .. } => u32::try_from(*value).ok()?,
        };
        out.push(b);
    }
    Some(Timeline {
        bucket_ns,
        start_unix_ns,
        sample_bucket: out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_model::*;

    fn meta() -> ProfileMeta {
        ProfileMeta {
            kind: ProfileKind::Cpu,
            source_format: SourceFormat::Pprof,
            fidelity: Fidelity::Native,
            duration_ns: None,
            captured_at_unix_ns: None,
            period: None,
        }
    }

    fn profile(labels: &[Option<(&str, bool, &str, i64)>]) -> Profile {
        let mut b = ProfileBuilder::new(
            vec![SampleType {
                name: "cpu".into(),
                unit: Unit::Nanoseconds,
            }],
            meta(),
        );
        for (i, l) in labels.iter().enumerate() {
            let f = b.function(&format!("f{i}"), "x", 1);
            let fr = b.frame(f, 1, 0);
            let st = b.stack(&[fr]);
            let ls = match l {
                None => vec![],
                Some((key, is_str, sval, nval)) => {
                    let key = b.intern(key);
                    let value = if *is_str {
                        LabelValue::Str(b.intern(sval))
                    } else {
                        LabelValue::Num {
                            value: *nval,
                            unit: None,
                        }
                    };
                    vec![Label { key, value }]
                }
            };
            b.add_sample(st, &[1], ls).unwrap();
        }
        b.finish()
    }

    #[test]
    fn string_and_numeric_labels_become_buckets() {
        let p = profile(&[
            Some((BUCKET_LABEL, true, "3", 0)),
            Some((BUCKET_LABEL, false, "", 7)),
        ]);
        let t = timeline_from_labels(&p, 20_000_000, 5).unwrap();
        assert_eq!(t.sample_bucket, vec![3, 7]);
        assert_eq!((t.bucket_ns, t.start_unix_ns), (20_000_000, 5));
        assert!(t.validate(2));
    }

    #[test]
    fn a_missing_or_bad_label_gives_no_timeline() {
        assert!(
            timeline_from_labels(&profile(&[Some((BUCKET_LABEL, true, "1", 0)), None]), 1, 0)
                .is_none()
        );
        assert!(
            timeline_from_labels(&profile(&[Some((BUCKET_LABEL, true, "x", 0))]), 1, 0).is_none()
        );
        assert!(
            timeline_from_labels(&profile(&[Some((BUCKET_LABEL, false, "", -4))]), 1, 0).is_none()
        );
        assert!(timeline_from_labels(&profile(&[Some(("other", true, "1", 0))]), 1, 0).is_none());
        assert!(
            timeline_from_labels(&profile(&[Some((BUCKET_LABEL, true, "1", 0))]), 0, 0).is_none()
        );
    }

    #[test]
    fn empty_profile_gives_an_empty_valid_timeline() {
        let t = timeline_from_labels(&profile(&[]), 10, 0).unwrap();
        assert!(t.validate(0));
    }
}
