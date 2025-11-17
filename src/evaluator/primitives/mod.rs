use super::{Environment, EvalError, Value};

mod collections;
mod logic;
mod numeric;
mod printer;
mod strings;

pub(crate) use collections::{eval_assoc, eval_contains, eval_disj, eval_dissoc, eval_get, eval_hash_map, eval_set, eval_subs, eval_vec};
pub(crate) use logic::{eval_logical_and, eval_logical_not, eval_logical_or};
pub(crate) use numeric::{eval_arithmetic_op, eval_comparison_op, eval_equal};
pub(crate) use printer::{eval_print, eval_printf, eval_println};
pub(crate) use strings::{eval_count, eval_str};
