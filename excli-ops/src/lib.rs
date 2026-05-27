//! excli-ops: Data operations — filter, sort, merge, aggregate, and clean.

pub mod filter;
pub mod sort;
pub mod aggregate;
pub mod merge;
pub mod clean;
pub mod transform;
pub mod search;


pub use filter::{filter_rows, FilterCondition, FilterOperator};
pub use sort::{sort_rows, SortOrder, SortSpec};
pub use aggregate::{aggregate, AggregateOp, Aggregation};
pub use merge::{merge_workbooks, JoinType};
pub use clean::{clean_trim, clean_case, clean_blanks, remove_duplicates, CaseType};
pub use transform::{transpose, concat_workbooks, ConcatAxis};
pub use search::{search_cells, replace_cells, SearchOptions, SearchResult};
