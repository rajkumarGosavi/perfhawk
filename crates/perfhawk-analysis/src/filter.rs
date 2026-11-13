use crate::transform::{apply, Transform};
use perfhawk_model::Timeline;
use perfhawk_model::{FuncId, Profile};
use regex::Regex;
use std::sync::Arc;

/// A half-open range `[from_bucket, to_bucket)` of a profile's timeline.
#[derive(Debug, Clone)]
pub struct Window {
    pub timeline: Arc<Timeline>,
    pub from_bucket: u32,
    pub to_bucket: u32,
}

impl Window {
    fn contains(&self, sample: usize) -> bool {
        self.timeline
            .sample_bucket
            .get(sample)
            .is_some_and(|&b| b >= self.from_bucket && b < self.to_bucket)
    }
}

/// pprof-compatible sample filters, matched against function names.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Keep only samples whose stack has a matching function.
    pub focus: Option<Regex>,
    /// Drop samples whose stack has a matching function.
    pub exclude: Option<Regex>,
    /// Remove matching frames from stacks, keeping the sample.
    pub hide: Option<Regex>,
    /// Stack transforms applied after exclude, focus and hide, in order.
    pub transforms: Vec<Transform>,
    /// Only samples inside this time window are considered.
    pub window: Option<Window>,
}

impl Filter {
    pub fn parse(
        focus: Option<&str>,
        exclude: Option<&str>,
        hide: Option<&str>,
    ) -> Result<Filter, regex::Error> {
        let c = |s: Option<&str>| s.map(Regex::new).transpose();
        Ok(Filter {
            focus: c(focus)?,
            exclude: c(exclude)?,
            hide: c(hide)?,
            transforms: Vec::new(),
            window: None,
        })
    }

    pub fn with_transforms(mut self, t: Vec<Transform>) -> Filter {
        self.transforms = t;
        self
    }

    pub fn with_window(mut self, w: Window) -> Filter {
        self.window = Some(w);
        self
    }

    /// True when no filter is set (every sample and frame is kept).
    pub fn is_empty(&self) -> bool {
        self.focus.is_none()
            && self.exclude.is_none()
            && self.hide.is_none()
            && self.transforms.is_empty()
            && self.window.is_none()
    }
}

/// Regex plus a per-function memo so each function name is tested once.
struct Cached<'a> {
    re: Option<&'a Regex>,
    memo: Vec<Option<bool>>,
}

impl<'a> Cached<'a> {
    fn new(re: &'a Option<Regex>, nfuncs: usize) -> Self {
        Cached {
            re: re.as_ref(),
            memo: vec![None; if re.is_some() { nfuncs } else { 0 }],
        }
    }

    fn matches(&mut self, p: &Profile, f: FuncId) -> bool {
        let Some(re) = self.re else { return false };
        let slot = &mut self.memo[f.0 as usize];
        *slot.get_or_insert_with(|| re.is_match(p.str(p.functions[f.0 as usize].name)))
    }
}

/// Calls `visit(functions_root_first, value)` for every sample that survives
/// the filter, after `hide` has been applied. Order: exclude, focus, hide.
pub(crate) fn for_each_sample(
    p: &Profile,
    sample_type: usize,
    filter: &Filter,
    mut visit: impl FnMut(&[FuncId], i64),
) {
    for_each_sample_indexed(p, sample_type, filter, |_, funcs, v| visit(funcs, v));
}

/// Like [`for_each_sample`], also passing the sample's index in `p.samples`.
pub(crate) fn for_each_sample_indexed(
    p: &Profile,
    sample_type: usize,
    filter: &Filter,
    mut visit: impl FnMut(usize, &[FuncId], i64),
) {
    if sample_type >= p.sample_types.len() {
        return;
    }
    let n = p.functions.len();
    let mut focus = Cached::new(&filter.focus, n);
    let mut exclude = Cached::new(&filter.exclude, n);
    let mut hide = Cached::new(&filter.hide, n);
    let mut funcs: Vec<FuncId> = Vec::new();
    for (i, s) in p.samples.iter().enumerate() {
        if let Some(w) = &filter.window {
            if !w.contains(i) {
                continue;
            }
        }
        funcs.clear();
        let mut cur = s.stack;
        while let Some(id) = cur {
            let node = &p.stacks[id.0 as usize];
            funcs.push(p.frames[node.frame.0 as usize].function);
            cur = node.parent;
        }
        funcs.reverse();
        if exclude.re.is_some() && funcs.iter().any(|&f| exclude.matches(p, f)) {
            continue;
        }
        if focus.re.is_some() && !funcs.iter().any(|&f| focus.matches(p, f)) {
            continue;
        }
        if hide.re.is_some() {
            funcs.retain(|&f| !hide.matches(p, f));
        }
        if !filter.transforms.iter().all(|t| apply(t, p, &mut funcs)) {
            continue;
        }
        visit(i, &funcs, p.value(i, sample_type));
    }
}
