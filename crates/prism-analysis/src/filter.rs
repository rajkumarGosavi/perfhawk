use prism_model::{FuncId, Profile};
use regex::Regex;

/// pprof-compatible sample filters, matched against function names.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Keep only samples whose stack has a matching function.
    pub focus: Option<Regex>,
    /// Drop samples whose stack has a matching function.
    pub exclude: Option<Regex>,
    /// Remove matching frames from stacks, keeping the sample.
    pub hide: Option<Regex>,
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
        })
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
    if sample_type >= p.sample_types.len() {
        return;
    }
    let n = p.functions.len();
    let mut focus = Cached::new(&filter.focus, n);
    let mut exclude = Cached::new(&filter.exclude, n);
    let mut hide = Cached::new(&filter.hide, n);
    let mut funcs: Vec<FuncId> = Vec::new();
    for (i, s) in p.samples.iter().enumerate() {
        funcs.clear();
        funcs.extend(
            p.stack_frames(s.stack)
                .into_iter()
                .map(|f| p.frames[f.0 as usize].function),
        );
        if exclude.re.is_some() && funcs.iter().any(|&f| exclude.matches(p, f)) {
            continue;
        }
        if focus.re.is_some() && !funcs.iter().any(|&f| focus.matches(p, f)) {
            continue;
        }
        if hide.re.is_some() {
            funcs.retain(|&f| !hide.matches(p, f));
        }
        visit(&funcs, p.value(i, sample_type));
    }
}
