//! Analysis engine: flame graph, inverted flame graph, top table and sandwich
//! views over a [`prism_model::Profile`], with pprof-compatible filters.

mod filter;
mod sandwich;
mod top;
mod tree;

pub use filter::Filter;
pub use sandwich::{sandwich, Sandwich};
pub use top::{top, TopRow};
pub use tree::{flame, inverted_flame, FlameNode};
