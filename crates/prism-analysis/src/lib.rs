//! Analysis engine: flame graph, inverted flame graph, top table and sandwich
//! views over a [`prism_model::Profile`], with pprof-compatible filters.

mod diff;
mod filter;
mod findings;
mod groups;
mod pivot;
mod sandwich;
mod top;
mod transform;
mod tree;

pub use diff::{
    diff_flame, diff_top, diff_top_sorted, match_sample_type, DiffError, DiffInput, DiffNode,
    DiffNormalize, DiffRow, DiffSort,
};
pub use filter::Filter;
pub use findings::{findings, Evidence, Finding, FindingAction, RuleConfig, Severity};
pub use groups::{groups, GroupBy, GroupRow};
pub use pivot::function_stats;
pub use sandwich::{sandwich, Sandwich};
pub use top::{filtered_total, top, TopRow, TopSort};
pub use transform::{package_of, parse_transforms, Transform, TransformError};
pub use tree::{flame, flame_with, inverted_flame, inverted_flame_with, FlameNode, FlameOptions};
