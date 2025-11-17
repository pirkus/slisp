use super::builtins::free_retained_slot;
use super::context::CompileContext;
use crate::ir::IRInstruction;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MapKeyLiteral {
    String(String),
    Keyword(String),
    Number(i64),
    Boolean(bool),
    Nil,
}

const TAG_NIL: i64 = 0;
const TAG_NUMBER: i64 = 1;
const TAG_BOOLEAN: i64 = 2;
const TAG_STRING: i64 = 3;
const TAG_VECTOR: i64 = 4;
const TAG_MAP: i64 = 5;
const TAG_KEYWORD: i64 = 6;
const TAG_SET: i64 = 7;
const TAG_ANY: i64 = 0xff;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ValueKind {
    Any,
    Number,
    Boolean,
    String,
    Keyword,
    Vector,
    Map,
    Set,
    Nil,
}

impl ValueKind {
    pub fn is_heap_kind(self) -> bool {
        matches!(self, ValueKind::String | ValueKind::Vector | ValueKind::Map | ValueKind::Set)
    }

    pub fn is_heap_clone_kind(self) -> bool {
        matches!(self, ValueKind::String | ValueKind::Keyword | ValueKind::Vector | ValueKind::Map | ValueKind::Set)
    }

    pub fn runtime_tag(self) -> i64 {
        match self {
            ValueKind::Nil => TAG_NIL,
            ValueKind::Number => TAG_NUMBER,
            ValueKind::Boolean => TAG_BOOLEAN,
            ValueKind::String => TAG_STRING,
            ValueKind::Keyword => TAG_KEYWORD,
            ValueKind::Vector => TAG_VECTOR,
            ValueKind::Map => TAG_MAP,
            ValueKind::Set => TAG_SET,
            ValueKind::Any => TAG_ANY,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HeapOwnership {
    None,
    Borrowed,
    Owned,
}

impl HeapOwnership {
    pub fn combine(self, other: Self) -> Self {
        use HeapOwnership::*;
        match (self, other) {
            (Owned, Owned) => Owned,
            (None, None) => None,
            (None, Borrowed) | (Borrowed, None) | (Borrowed, Borrowed) => Borrowed,
            _ => Borrowed,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedSlot {
    pub slot: usize,
    pub key: Option<MapKeyLiteral>,
    pub kind: ValueKind,
    pub dependents: Vec<RetainedSlot>,
}

pub type MapValueTypes = HashMap<MapKeyLiteral, ValueKind>;

pub struct CompileResult {
    pub instructions: Vec<IRInstruction>,
    pub kind: ValueKind,
    pub heap_ownership: HeapOwnership,
    pub map_value_types: Option<MapValueTypes>,
    pub retained_slots: Vec<RetainedSlot>,
}

impl CompileResult {
    pub fn with_instructions(instructions: Vec<IRInstruction>, kind: ValueKind) -> Self {
        Self {
            instructions,
            kind,
            heap_ownership: HeapOwnership::None,
            map_value_types: None,
            retained_slots: Vec::new(),
        }
    }

    pub fn with_heap_ownership(mut self, ownership: HeapOwnership) -> Self {
        self.heap_ownership = ownership;
        self
    }

    pub fn with_map_value_types(mut self, types: Option<MapValueTypes>) -> Self {
        self.map_value_types = types;
        self
    }

    pub fn with_retained_slots(mut self, slots: Vec<RetainedSlot>) -> Self {
        self.retained_slots = slots;
        self
    }

    pub fn take_retained_slots(&mut self) -> Vec<RetainedSlot> {
        std::mem::take(&mut self.retained_slots)
    }

    pub fn free_retained_slots(&mut self, instructions: &mut Vec<IRInstruction>, context: &mut CompileContext) {
        for slot in self.retained_slots.drain(..) {
            free_retained_slot(slot, instructions, context);
        }
    }
}
