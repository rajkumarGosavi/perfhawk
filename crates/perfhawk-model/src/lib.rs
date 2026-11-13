//! Unified profile model shared by importers, analysis, store and server.

mod timeline;
pub use timeline::Timeline;

use rustc_hash::FxHashMap as HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StrId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FuncId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FrameId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StackId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileKind {
    Cpu,
    Wall,
    Memory,
    Lock,
    Blocking,
    BlockIo,
    Threads,
    Unknown,
}

impl ProfileKind {
    pub const ALL: [ProfileKind; 8] = [
        ProfileKind::Cpu,
        ProfileKind::Wall,
        ProfileKind::Memory,
        ProfileKind::Lock,
        ProfileKind::Blocking,
        ProfileKind::BlockIo,
        ProfileKind::Threads,
        ProfileKind::Unknown,
    ];

    /// The serialized (snake_case) name.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProfileKind::Cpu => "cpu",
            ProfileKind::Wall => "wall",
            ProfileKind::Memory => "memory",
            ProfileKind::Lock => "lock",
            ProfileKind::Blocking => "blocking",
            ProfileKind::BlockIo => "block_io",
            ProfileKind::Threads => "threads",
            ProfileKind::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Option<ProfileKind> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

impl std::fmt::Display for ProfileKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Unit {
    Nanoseconds,
    Bytes,
    Count,
    Other(String),
}

impl Unit {
    pub fn parse(s: &str) -> Unit {
        match s {
            "nanoseconds" | "ns" => Unit::Nanoseconds,
            "bytes" => Unit::Bytes,
            "count" | "samples" | "" => Unit::Count,
            other => Unit::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fidelity {
    Native,
    SampledExternal,
    Derived,
    Approximate,
    Unknown,
}

impl Fidelity {
    /// The serialized (snake_case) name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Fidelity::Native => "native",
            Fidelity::SampledExternal => "sampled_external",
            Fidelity::Derived => "derived",
            Fidelity::Approximate => "approximate",
            Fidelity::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Option<Fidelity> {
        [
            Fidelity::Native,
            Fidelity::SampledExternal,
            Fidelity::Derived,
            Fidelity::Approximate,
            Fidelity::Unknown,
        ]
        .into_iter()
        .find(|f| f.as_str() == s)
    }
}

impl std::fmt::Display for Fidelity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    Pprof,
    Collapsed,
    Speedscope,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleType {
    pub name: String,
    pub unit: Unit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Function {
    pub name: StrId,
    pub file: StrId,
    pub start_line: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub function: FuncId,
    pub line: u32,
    pub address: u64,
}

/// Prefix-tree node, root-first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackNode {
    pub parent: Option<StackId>,
    pub frame: FrameId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Label {
    pub key: StrId,
    pub value: LabelValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LabelValue {
    Str(StrId),
    Num { value: i64, unit: Option<StrId> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// `None` = empty stack.
    pub stack: Option<StackId>,
    pub labels: Vec<Label>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileMeta {
    pub kind: ProfileKind,
    pub source_format: SourceFormat,
    pub fidelity: Fidelity,
    pub duration_ns: Option<u64>,
    pub captured_at_unix_ns: Option<i64>,
    pub period: Option<(SampleType, i64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub strings: Vec<String>,
    pub functions: Vec<Function>,
    pub frames: Vec<Frame>,
    pub stacks: Vec<StackNode>,
    pub sample_types: Vec<SampleType>,
    pub default_sample_type: usize,
    pub samples: Vec<Sample>,
    /// Row-major: `values.len() == samples.len() * sample_types.len()`.
    pub values: Vec<i64>,
    pub meta: ProfileMeta,
}

impl Profile {
    pub fn str(&self, id: StrId) -> &str {
        &self.strings[id.0 as usize]
    }

    pub fn value(&self, sample: usize, sample_type: usize) -> i64 {
        debug_assert!(sample_type < self.sample_types.len());
        self.values[sample * self.sample_types.len() + sample_type]
    }

    /// Frames of a stack, root-first. Iterative: safe for arbitrarily deep stacks.
    pub fn stack_frames(&self, stack: Option<StackId>) -> Vec<FrameId> {
        let mut out = Vec::new();
        let mut cur = stack;
        while let Some(id) = cur {
            let node = &self.stacks[id.0 as usize];
            out.push(node.frame);
            cur = node.parent;
        }
        out.reverse();
        out
    }

    pub fn total(&self, sample_type: usize) -> i64 {
        let n = self.sample_types.len();
        (0..self.samples.len()).fold(0i64, |acc, i| {
            acc.saturating_add(self.values[i * n + sample_type])
        })
    }

    pub fn sample_type_index(&self, name: &str) -> Option<usize> {
        self.sample_types.iter().position(|t| t.name == name)
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ModelError {
    #[error("expected {expected} values per sample, got {got}")]
    ValueCountMismatch { expected: usize, got: usize },
}

const NO_HINT: (FrameId, StackId) = (FrameId(u32::MAX), StackId(u32::MAX));

pub struct ProfileBuilder {
    strings: Vec<String>,
    string_ids: HashMap<String, StrId>,
    functions: Vec<Function>,
    function_ids: HashMap<(StrId, StrId, u32), FuncId>,
    frames: Vec<Frame>,
    frame_ids: HashMap<(FuncId, u32, u64), FrameId>,
    stacks: Vec<StackNode>,
    stack_ids: HashMap<(Option<StackId>, FrameId), StackId>,
    /// Per stack node (and one slot for the root, at index 0 of `root_hint`):
    /// the most recently taken child. Stacks that share a prefix usually take
    /// the same child, so this skips most hash lookups.
    child_hint: Vec<(FrameId, StackId)>,
    root_hint: (FrameId, StackId),
    sample_types: Vec<SampleType>,
    default_sample_type: usize,
    samples: Vec<Sample>,
    values: Vec<i64>,
    meta: ProfileMeta,
}

impl ProfileBuilder {
    pub fn new(sample_types: Vec<SampleType>, meta: ProfileMeta) -> Self {
        let mut string_ids = HashMap::default();
        string_ids.insert(String::new(), StrId(0));
        ProfileBuilder {
            strings: vec![String::new()],
            string_ids,
            functions: Vec::new(),
            function_ids: HashMap::default(),
            frames: Vec::new(),
            frame_ids: HashMap::default(),
            stacks: Vec::new(),
            stack_ids: HashMap::default(),
            child_hint: Vec::new(),
            root_hint: NO_HINT,
            sample_types,
            default_sample_type: 0,
            samples: Vec::new(),
            values: Vec::new(),
            meta,
        }
    }

    pub fn intern(&mut self, s: &str) -> StrId {
        if let Some(&id) = self.string_ids.get(s) {
            return id;
        }
        let id = StrId(self.strings.len() as u32);
        self.strings.push(s.to_string());
        self.string_ids.insert(s.to_string(), id);
        id
    }

    pub fn function(&mut self, name: &str, file: &str, start_line: u32) -> FuncId {
        let key = (self.intern(name), self.intern(file), start_line);
        if let Some(&id) = self.function_ids.get(&key) {
            return id;
        }
        let id = FuncId(self.functions.len() as u32);
        self.functions.push(Function {
            name: key.0,
            file: key.1,
            start_line,
        });
        self.function_ids.insert(key, id);
        id
    }

    pub fn frame(&mut self, f: FuncId, line: u32, address: u64) -> FrameId {
        let key = (f, line, address);
        if let Some(&id) = self.frame_ids.get(&key) {
            return id;
        }
        let id = FrameId(self.frames.len() as u32);
        self.frames.push(Frame {
            function: f,
            line,
            address,
        });
        self.frame_ids.insert(key, id);
        id
    }

    /// Interns a root-first stack; returns the leaf node, or `None` for an empty stack.
    pub fn stack(&mut self, root_first: &[FrameId]) -> Option<StackId> {
        let mut parent: Option<StackId> = None;
        for &frame in root_first {
            let hint = match parent {
                Some(p) => &mut self.child_hint[p.0 as usize],
                None => &mut self.root_hint,
            };
            if hint.0 == frame && hint.1 != NO_HINT.1 {
                parent = Some(hint.1);
                continue;
            }
            let key = (parent, frame);
            let id = match self.stack_ids.get(&key) {
                Some(&id) => id,
                None => {
                    let id = StackId(self.stacks.len() as u32);
                    self.stacks.push(StackNode { parent, frame });
                    self.child_hint.push(NO_HINT);
                    self.stack_ids.insert(key, id);
                    id
                }
            };
            match parent {
                Some(p) => self.child_hint[p.0 as usize] = (frame, id),
                None => self.root_hint = (frame, id),
            }
            parent = Some(id);
        }
        parent
    }

    pub fn add_sample(
        &mut self,
        stack: Option<StackId>,
        values: &[i64],
        labels: Vec<Label>,
    ) -> Result<(), ModelError> {
        let expected = self.sample_types.len();
        if values.len() != expected {
            return Err(ModelError::ValueCountMismatch {
                expected,
                got: values.len(),
            });
        }
        self.samples.push(Sample { stack, labels });
        self.values.extend_from_slice(values);
        Ok(())
    }

    pub fn set_default_sample_type(&mut self, idx: usize) {
        self.default_sample_type = idx;
    }

    pub fn finish(self) -> Profile {
        Profile {
            strings: self.strings,
            functions: self.functions,
            frames: self.frames,
            stacks: self.stacks,
            sample_types: self.sample_types,
            default_sample_type: self.default_sample_type,
            samples: self.samples,
            values: self.values,
            meta: self.meta,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(name: &str) -> SampleType {
        SampleType {
            name: name.into(),
            unit: Unit::Count,
        }
    }
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
    fn chain(b: &mut ProfileBuilder, names: &[&str]) -> Option<StackId> {
        let frames: Vec<FrameId> = names
            .iter()
            .map(|n| {
                let f = b.function(n, "f.go", 1);
                b.frame(f, 1, 0)
            })
            .collect();
        b.stack(&frames)
    }

    #[test]
    fn builder_dedupes_strings_functions_frames_and_stack_prefixes() {
        let mut b = ProfileBuilder::new(vec![st("samples")], meta());
        let s0 = chain(&mut b, &["main", "a", "b"]);
        let s1 = chain(&mut b, &["main", "a", "c"]);
        b.add_sample(s0, &[1], vec![]).unwrap();
        b.add_sample(s1, &[1], vec![]).unwrap();
        let p = b.finish();
        assert_eq!(p.functions.len(), 4);
        assert_eq!(p.frames.len(), 4);
        assert_eq!(p.stacks.len(), 4);
        assert_eq!(p.strings[0], "");
    }

    #[test]
    fn interleaved_stacks_intern_to_the_same_nodes() {
        let mut b = ProfileBuilder::new(vec![st("samples")], meta());
        let ab = chain(&mut b, &["a", "b"]);
        let ac = chain(&mut b, &["a", "c"]);
        let c = chain(&mut b, &["c"]);
        assert_eq!(chain(&mut b, &["a", "b"]), ab);
        assert_eq!(chain(&mut b, &["a", "c"]), ac);
        assert_eq!(chain(&mut b, &["c"]), c);
        assert_ne!(ab, ac);
        assert_eq!(b.finish().stacks.len(), 4);
    }

    #[test]
    fn total_saturates_instead_of_overflowing() {
        let mut b = ProfileBuilder::new(vec![st("samples")], meta());
        let s0 = chain(&mut b, &["main"]);
        b.add_sample(s0, &[i64::MAX / 2 + 1], vec![]).unwrap();
        b.add_sample(s0, &[i64::MAX / 2 + 1], vec![]).unwrap();
        assert_eq!(b.finish().total(0), i64::MAX);
    }

    #[test]
    fn stack_frames_returns_root_first() {
        let mut b = ProfileBuilder::new(vec![st("samples")], meta());
        let s0 = chain(&mut b, &["main", "a", "b"]);
        b.add_sample(s0, &[1], vec![]).unwrap();
        b.add_sample(None, &[1], vec![]).unwrap();
        let p = b.finish();
        let names: Vec<&str> = p
            .stack_frames(p.samples[0].stack)
            .iter()
            .map(|f| p.str(p.functions[p.frames[f.0 as usize].function.0 as usize].name))
            .collect();
        assert_eq!(names, ["main", "a", "b"]);
        assert!(p.stack_frames(None).is_empty());
    }

    #[test]
    fn values_are_row_major_and_total_sums() {
        let mut b = ProfileBuilder::new(vec![st("samples"), st("cpu")], meta());
        b.add_sample(None, &[1, 10], vec![]).unwrap();
        b.add_sample(None, &[2, 20], vec![]).unwrap();
        let p = b.finish();
        assert_eq!(p.value(1, 1), 20);
        assert_eq!(p.total(0), 3);
        assert_eq!(p.sample_type_index("cpu"), Some(1));
        assert_eq!(p.sample_type_index("nope"), None);
    }

    #[test]
    fn add_sample_rejects_wrong_value_count() {
        let mut b = ProfileBuilder::new(vec![st("samples"), st("cpu")], meta());
        let r = b.add_sample(None, &[1], vec![]);
        assert_eq!(
            r,
            Err(ModelError::ValueCountMismatch {
                expected: 2,
                got: 1
            })
        );
    }

    #[test]
    fn deep_stack_10000_frames_does_not_overflow() {
        let mut b = ProfileBuilder::new(vec![st("samples")], meta());
        let f = b.function("rec", "f.go", 1);
        let fr = b.frame(f, 1, 0);
        let frames = vec![fr; 10_000];
        let s = b.stack(&frames);
        b.add_sample(s, &[1], vec![]).unwrap();
        let p = b.finish();
        assert_eq!(p.stack_frames(p.samples[0].stack).len(), 10_000);
    }

    #[test]
    fn unit_parse() {
        assert_eq!(Unit::parse("nanoseconds"), Unit::Nanoseconds);
        assert_eq!(Unit::parse("ns"), Unit::Nanoseconds);
        assert_eq!(Unit::parse("bytes"), Unit::Bytes);
        assert_eq!(Unit::parse(""), Unit::Count);
        assert_eq!(Unit::parse("furlongs"), Unit::Other("furlongs".into()));
    }
}
