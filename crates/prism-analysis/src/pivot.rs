use crate::filter::Filter;
use crate::top::{top, TopRow, TopSort};
use prism_model::Profile;

/// The [`TopRow`] of the function named exactly `name` (the first by id when
/// several files share the name), or `None` when it never appears.
pub fn function_stats(
    p: &Profile,
    sample_type: usize,
    filter: &Filter,
    name: &str,
) -> Option<TopRow> {
    top(p, sample_type, filter, usize::MAX, TopSort::SelfValue)
        .into_iter()
        .filter(|r| r.name == name)
        .min_by_key(|r| r.func_id)
}
