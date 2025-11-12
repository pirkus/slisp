/// Liveness planner for heap-owned locals.
///
/// The compiler tracks which stack slots hold heap pointers (strings/maps/vectors/sets). Rather
/// than eagerly call `_free` after every instruction, we build a `LivenessPlan` that records the
/// *last use* of each tracked slot and inserts `FreeLocal` instructions at those points. The
/// planner works in two phases:
///
/// 1. `compute_liveness_plan` walks the IR (including branches) and determines where frees should
///    be inserted, plus which slots are guaranteed to be freed along *all* paths exiting the
///    region. For straight-line code we just track the last consumer; for branches we recurse into
///    the then/else blocks, taking the intersection of their `freed_everywhere` sets.
/// 2. `apply_liveness_plan` rewrites the IR, splicing in the frees and patching jump offsets.
///
/// Any slots still owned after liveness gets a plan are freed by the surrounding scope
/// (e.g. `compile_let`), but most of the work happens here so we avoid double-frees and ensure
/// borrowed values are not released prematurely.
use crate::ir::IRInstruction;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default, Clone)]
pub struct LivenessPlan {
    pub insert_after: HashMap<usize, Vec<usize>>, // instruction index -> slots to free after executing it
    pub freed_everywhere: HashSet<usize>,         // slots guaranteed freed along all paths exiting the analysed range
}

pub fn compute_liveness_plan(instructions: &[IRInstruction], tracked_slots: &HashSet<usize>) -> LivenessPlan {
    plan_range(instructions, tracked_slots, 0, instructions.len())
}

pub fn apply_liveness_plan<F>(original: Vec<IRInstruction>, plan: &LivenessPlan, mut emit_free: F) -> Vec<IRInstruction>
where
    F: FnMut(&mut Vec<IRInstruction>, usize),
{
    if plan.insert_after.is_empty() {
        return original;
    }

    let mut new_instructions = Vec::with_capacity(original.len() + plan.insert_after.len());
    let mut index_map = Vec::with_capacity(original.len());

    for (idx, inst) in original.into_iter().enumerate() {
        index_map.push(new_instructions.len());
        new_instructions.push(inst);
        if let Some(slots) = plan.insert_after.get(&idx) {
            for slot in slots {
                emit_free(&mut new_instructions, *slot);
            }
        }
    }

    let original_len = index_map.len();
    let final_len = new_instructions.len();

    // Adjust jump targets to account for inserted instructions
    for inst in &mut new_instructions {
        match inst {
            IRInstruction::Jump(target) | IRInstruction::JumpIfZero(target) => {
                if *target == original_len {
                    *target = final_len;
                } else if let Some(&mapped) = index_map.get(*target) {
                    *target = mapped;
                }
            }
            _ => {}
        }
    }

    new_instructions
}

fn plan_range(instructions: &[IRInstruction], tracked_slots: &HashSet<usize>, start: usize, end: usize) -> LivenessPlan {
    if tracked_slots.is_empty() || start >= end {
        return LivenessPlan::default();
    }

    if let Some((jump_if_idx, else_start)) = find_branch(instructions, start, end) {
        let jump_idx = else_start.saturating_sub(1);
        if jump_idx >= end {
            return plan_linear_range(instructions, tracked_slots, start, end);
        }

        if let IRInstruction::Jump(end_pos) = instructions[jump_idx] {
            let mut result = LivenessPlan::default();

            let prefix_plan = plan_range(instructions, tracked_slots, start, jump_if_idx + 1);
            merge_plan(&mut result, prefix_plan.clone(), true);

            let remaining_after_prefix: HashSet<usize> = tracked_slots.iter().filter(|slot| !result.freed_everywhere.contains(slot)).copied().collect();

            if !remaining_after_prefix.is_empty() {
                let then_plan = plan_range(instructions, &remaining_after_prefix, jump_if_idx + 1, else_start);
                merge_plan(&mut result, then_plan.clone(), false);

                let else_plan = plan_range(instructions, &remaining_after_prefix, else_start, end_pos);
                merge_plan(&mut result, else_plan.clone(), false);

                let branch_freed: HashSet<usize> = then_plan.freed_everywhere.intersection(&else_plan.freed_everywhere).copied().collect();

                result.freed_everywhere.extend(branch_freed.iter().copied());

                let remaining_after_branch: HashSet<usize> = remaining_after_prefix.into_iter().filter(|slot| !result.freed_everywhere.contains(slot)).collect();

                if !remaining_after_branch.is_empty() {
                    let suffix_plan = plan_range(instructions, &remaining_after_branch, end_pos, end);
                    merge_plan(&mut result, suffix_plan, true);
                }
            } else if end_pos < end {
                let suffix_plan = plan_range(instructions, &HashSet::new(), end_pos, end);
                merge_plan(&mut result, suffix_plan, true);
            }

            return result;
        }
    }

    plan_linear_range(instructions, tracked_slots, start, end)
}

fn plan_linear_range(instructions: &[IRInstruction], tracked_slots: &HashSet<usize>, start: usize, end: usize) -> LivenessPlan {
    if tracked_slots.is_empty() || start >= end {
        return LivenessPlan::default();
    }

    let mut plan = LivenessPlan::default();

    let slots_used = collect_slot_usage(instructions, tracked_slots, start, end);

    if slots_used.is_empty() {
        return plan;
    }

    let last_use_map = collect_last_uses_straight_line(instructions, &slots_used, start, end);

    for (slot, idx) in last_use_map {
        plan.insert_after.entry(idx).or_default().push(slot);
        plan.freed_everywhere.insert(slot);
    }

    plan
}

fn merge_plan(target: &mut LivenessPlan, other: LivenessPlan, sequential: bool) {
    for (idx, slots) in other.insert_after {
        target.insert_after.entry(idx).or_default().extend(slots);
    }
    if sequential {
        target.freed_everywhere.extend(other.freed_everywhere);
    }
}

fn find_branch(instructions: &[IRInstruction], start: usize, end: usize) -> Option<(usize, usize)> {
    for idx in start..end {
        if let IRInstruction::JumpIfZero(target) = instructions[idx] {
            if target > idx && target <= end {
                return Some((idx, target));
            }
        }
    }
    None
}

fn collect_slot_usage(instructions: &[IRInstruction], tracked: &HashSet<usize>, start: usize, end: usize) -> HashSet<usize> {
    let mut used = HashSet::new();
    for idx in start..end {
        match instructions[idx] {
            IRInstruction::LoadLocal(slot) | IRInstruction::PushLocalAddress(slot) => {
                if tracked.contains(&slot) {
                    used.insert(slot);
                }
            }
            _ => {}
        }
    }
    used
}

#[derive(Clone, Copy)]
enum StackEntry {
    LocalValue(usize),
    LocalAddress(usize),
    Other,
}

fn collect_last_uses_straight_line(instructions: &[IRInstruction], tracked: &HashSet<usize>, start: usize, end: usize) -> HashMap<usize, usize> {
    let mut stack: Vec<StackEntry> = Vec::new();
    let mut last_use: HashMap<usize, usize> = HashMap::new();

    for offset in 0..(end - start) {
        let idx = start + offset;
        match instructions[idx] {
            IRInstruction::LoadLocal(slot) => {
                if tracked.contains(&slot) {
                    stack.push(StackEntry::LocalValue(slot));
                } else {
                    stack.push(StackEntry::Other);
                }
            }
            IRInstruction::PushLocalAddress(slot) => {
                if tracked.contains(&slot) {
                    stack.push(StackEntry::LocalAddress(slot));
                } else {
                    stack.push(StackEntry::Other);
                }
            }
            IRInstruction::LoadParam(_) | IRInstruction::Push(_) | IRInstruction::PushString(_) | IRInstruction::Allocate(_) => {
                stack.push(StackEntry::Other);
            }
            IRInstruction::StoreLocal(_) => {
                stack.pop();
            }
            IRInstruction::RuntimeCall(_, arg_count) | IRInstruction::Call(_, arg_count) => {
                consume_stack_entries(&mut stack, arg_count, &mut last_use, idx, tracked);
                stack.push(StackEntry::Other);
            }
            IRInstruction::Add
            | IRInstruction::Sub
            | IRInstruction::Mul
            | IRInstruction::Div
            | IRInstruction::Equal
            | IRInstruction::Less
            | IRInstruction::Greater
            | IRInstruction::LessEqual
            | IRInstruction::GreaterEqual => {
                consume_stack_entries(&mut stack, 2, &mut last_use, idx, tracked);
                stack.push(StackEntry::Other);
            }
            IRInstruction::Not | IRInstruction::Free => {
                consume_stack_entries(&mut stack, 1, &mut last_use, idx, tracked);
                stack.push(StackEntry::Other);
            }
            IRInstruction::Jump(_) | IRInstruction::JumpIfZero(_) => {}
            IRInstruction::Return => {
                stack.pop();
            }
            IRInstruction::FreeLocal(_) | IRInstruction::FreeLocalWithRuntime(_, _) | IRInstruction::DefineFunction(_, _, _) | IRInstruction::InitHeap => {}
        }
    }

    last_use
}

fn consume_stack_entries(stack: &mut Vec<StackEntry>, count: usize, last_use: &mut HashMap<usize, usize>, idx: usize, tracked: &HashSet<usize>) {
    for _ in 0..count {
        match stack.pop() {
            Some(StackEntry::LocalValue(slot)) | Some(StackEntry::LocalAddress(slot)) => {
                if tracked.contains(&slot) {
                    last_use.insert(slot, idx);
                }
            }
            Some(StackEntry::Other) | None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_line_plan_frees_last_use() {
        let instructions = vec![
            IRInstruction::LoadLocal(0),
            IRInstruction::RuntimeCall("foo".to_string(), 1),
            IRInstruction::LoadLocal(1),
            IRInstruction::RuntimeCall("bar".to_string(), 1),
            IRInstruction::Return,
        ];
        let tracked: HashSet<usize> = [0, 1].into_iter().collect();
        let plan = compute_liveness_plan(&instructions, &tracked);
        assert_eq!(plan.insert_after.get(&1).map(|slots| slots.as_slice()), Some(&[0][..]));
        assert_eq!(plan.insert_after.get(&3).map(|slots| slots.as_slice()), Some(&[1][..]));
        assert!(plan.freed_everywhere.contains(&0));
        assert!(plan.freed_everywhere.contains(&1));
    }

    #[test]
    fn branch_only_marks_slots_freed_on_both_paths() {
        let instructions = vec![
            IRInstruction::LoadLocal(0),
            IRInstruction::JumpIfZero(4),
            IRInstruction::RuntimeCall("foo".to_string(), 1),
            IRInstruction::Jump(6),
            IRInstruction::LoadLocal(1),
            IRInstruction::RuntimeCall("bar".to_string(), 1),
            IRInstruction::Return,
        ];
        let tracked: HashSet<usize> = [0, 1].into_iter().collect();
        let plan = compute_liveness_plan(&instructions, &tracked);
        assert!(plan.freed_everywhere.is_empty());
        // Only one branch frees slot 0/1 respectively, so they should not appear in
        // `freed_everywhere`.
        assert!(plan.freed_everywhere.is_empty());
    }

    #[test]
    fn unused_tracked_slots_yield_empty_plan() {
        let instructions = vec![IRInstruction::Push(1), IRInstruction::Return];
        let tracked: HashSet<usize> = [0].into_iter().collect();
        let plan = compute_liveness_plan(&instructions, &tracked);
        assert!(plan.insert_after.is_empty());
        assert!(plan.freed_everywhere.is_empty());
    }
}
