//! Deterministic generator of large synthetic pprof profiles for perf tests.
//!
//! Model: 50 000 functions (one location each) drawn Zipf-distributed. Call
//! paths come from a tree of 10 000 "templates" (each forks a parent's prefix,
//! like handlers under a shared server loop) and 512 recursion patterns (a
//! template prefix plus a cycle of 1–4 uniformly chosen functions, picked
//! uniformly so no single recursion dominates). 90 % of samples are 5–200
//! frames deep and follow a Zipf-chosen template; 10 % are 1 000–5 000 deep
//! and follow a recursion pattern.
//! Every sample's last 1–3 frames are fresh Zipf draws (leaf variety). Two
//! sample types: `samples/count` and `cpu/nanoseconds`. Output is uncompressed
//! protobuf, written streaming until it reaches the requested size.

use perfhawk_import::pprof::proto;
use prost::Message;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

const FUNCS: usize = 50_000;
const TEMPLATES: usize = 10_000;
const TEMPLATE_LEN: usize = 200;
const RECURSIONS: usize = 512;
const RECURSION_LEN: usize = 5_000;
const PERIOD_NS: i64 = 10_000_000;

/// splitmix64: tiny, deterministic, good enough for synthetic data.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n`.
    fn below(&mut self, n: usize) -> usize {
        (((self.next() >> 11) as u128 * n as u128) >> 53) as usize
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Zipf(s = 1) over ranks `0..n` via an inverse-CDF table.
struct Zipf(Vec<f64>);

impl Zipf {
    fn new(n: usize) -> Self {
        let mut acc = 0.0;
        let mut cdf: Vec<f64> = (1..=n)
            .map(|k| {
                acc += 1.0 / k as f64;
                acc
            })
            .collect();
        let total = acc;
        cdf.iter_mut().for_each(|c| *c /= total);
        Zipf(cdf)
    }

    fn sample(&self, rng: &mut Rng) -> usize {
        let u = rng.unit();
        self.0.partition_point(|&c| c < u).min(self.0.len() - 1)
    }
}

fn header() -> proto::Profile {
    let mut strings: Vec<Vec<u8>> = vec![Vec::new()];
    let mut push = |s: String| {
        strings.push(s.into_bytes());
        (strings.len() - 1) as i64
    };
    let sample_type = vec![
        proto::ValueType {
            r#type: push("samples".into()),
            unit: push("count".into()),
        },
        proto::ValueType {
            r#type: push("cpu".into()),
            unit: push("nanoseconds".into()),
        },
    ];
    let mapping_file = push("/usr/local/bin/service".into());
    let files: Vec<i64> = (0..FUNCS / 10)
        .map(|i| {
            push(format!(
                "github.com/acme/svc{}/pkg{}/file{i}.go",
                i % 50,
                i % 997
            ))
        })
        .collect();
    let function = (0..FUNCS)
        .map(|i| proto::Function {
            id: i as u64 + 1,
            name: push(format!(
                "github.com/acme/svc{}/pkg{}.Func{i}",
                i % 50,
                i % 997
            )),
            system_name: 0,
            filename: files[i / 10],
            start_line: (i % 500) as i64 + 1,
        })
        .collect();
    let location = (0..FUNCS)
        .map(|i| proto::Location {
            id: i as u64 + 1,
            mapping_id: 1,
            address: 0x40_0000 + i as u64 * 16,
            line: vec![proto::Line {
                function_id: i as u64 + 1,
                line: (i % 500) as i64 + 5,
                column: 0,
            }],
            is_folded: false,
        })
        .collect();
    let period_type = Some(sample_type[1].clone());
    proto::Profile {
        sample_type,
        mapping: vec![proto::Mapping {
            id: 1,
            memory_start: 0x40_0000,
            memory_limit: 0x40_0000 + FUNCS as u64 * 16,
            filename: mapping_file,
            has_functions: true,
            has_filenames: true,
            has_line_numbers: true,
            ..Default::default()
        }],
        location,
        function,
        string_table: strings,
        time_nanos: 1_700_000_000_000_000_000,
        duration_nanos: 60_000_000_000,
        period_type,
        period: PERIOD_NS,
        ..Default::default()
    }
}

/// Writes an uncompressed pprof of at least `target_mb` MiB to `out`.
/// Identical `seed`s produce identical files.
pub fn generate(out: &Path, target_mb: u64, seed: u64) -> std::io::Result<()> {
    let mut rng = Rng(seed);
    let funcs = Zipf::new(FUNCS);
    let draw = |rng: &mut Rng| funcs.sample(rng) as u32;

    // Template tree: each template copies a prefix of an earlier one.
    let mut templates: Vec<Vec<u32>> = Vec::with_capacity(TEMPLATES);
    let root: Vec<u32> = (0..3)
        .chain((3..TEMPLATE_LEN).map(|_| draw(&mut rng)))
        .collect();
    templates.push(root);
    for t in 1..TEMPLATES {
        let parent = rng.below(t);
        let fork = 3 + rng.below(60);
        let mut path = templates[parent][..fork].to_vec();
        path.extend((fork..TEMPLATE_LEN).map(|_| draw(&mut rng)));
        templates.push(path);
    }
    let recursions: Vec<Vec<u32>> = (0..RECURSIONS)
        .map(|_| {
            let base = &templates[rng.below(TEMPLATES)];
            let prefix = 5 + rng.below(25);
            let cycle: Vec<u32> = (0..1 + rng.below(4))
                .map(|_| rng.below(FUNCS) as u32)
                .collect();
            let mut path = base[..prefix].to_vec();
            path.extend(cycle.iter().cycle().take(RECURSION_LEN - prefix));
            path
        })
        .collect();
    let pick_template = Zipf::new(TEMPLATES);

    let mut w = BufWriter::with_capacity(1 << 20, File::create(out)?);
    let head = header().encode_to_vec();
    w.write_all(&head)?;
    let target = target_mb.saturating_mul(1 << 20);
    let mut written = head.len() as u64;
    let mut sample = proto::Sample::default();
    let mut buf = Vec::with_capacity(64 * 1024);
    while written < target {
        let (base, depth) = if rng.below(10) == 0 {
            (&recursions[rng.below(RECURSIONS)], 1_000 + rng.below(4_001))
        } else {
            (
                &templates[pick_template.sample(&mut rng)],
                5 + rng.below(196),
            )
        };
        let tail = 1 + rng.below(3);
        sample.location_id.clear();
        // pprof order is leaf-first.
        for _ in 0..tail {
            sample.location_id.push(draw(&mut rng) as u64 + 1);
        }
        sample
            .location_id
            .extend(base[..depth - tail].iter().rev().map(|&f| f as u64 + 1));
        let count = 1 + rng.below(5) as i64;
        sample.value.clear();
        sample.value.extend([count, count * PERIOD_NS]);
        buf.clear();
        prost::encoding::message::encode(2, &sample, &mut buf);
        w.write_all(&buf)?;
        written += buf.len() as u64;
    }
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_profile_is_deterministic_and_decodes() {
        let dir = std::env::temp_dir().join(format!("genprof-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.pb"), dir.join("b.pb"));
        generate(&a, 4, 7).unwrap();
        generate(&b, 4, 7).unwrap();
        let (ba, bb) = (std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(ba.len() as u64 >= 4 << 20);
        assert_eq!(ba, bb);
        let p = proto::Profile::decode(&ba[..]).unwrap();
        assert_eq!(p.function.len(), FUNCS);
        assert_eq!(p.sample_type.len(), 2);
        assert!(!p.sample.is_empty());
        for s in &p.sample {
            let d = s.location_id.len();
            assert!(
                (5..=200).contains(&d) || (1_000..=5_000).contains(&d),
                "{d}"
            );
            assert_eq!(s.value.len(), 2);
        }
    }
}
