use rustc_middle::mir::{BasicBlock, LocalDecl, Place};
use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::interval_analysis::range::Range;

trait BasicIntervalTrait<'tcx, T> {
    // fn get_value_id(&self) -> IntervalId;
    fn get_range(&self) -> &Range<T>;
    fn set_range(&mut self, new_range: Range<T>);
    fn print(&self);
}

#[derive(Debug, Clone)]
pub struct BasicInterval<'tcx, T> {
    range: Range<T>,
}

impl<'tcx, T> BasicInterval<'tcx, T> {
    pub fn new(range: Range<T>) -> Self {
        Self { range }
    }
}

impl<'tcx, T> BasicIntervalTrait<'tcx, T> for BasicInterval<'tcx, T> {
    // fn get_value_id(&self) -> IntervalId {
    //     IntervalId::BasicIntervalId
    // }

    fn get_range(&self) -> &Range {
        &self.range
    }

    fn set_range(&mut self, new_range: Range) {
        self.range = new_range;
        if self.range.get_lower() > self.range.get_upper() {
            self.range.set_empty();
        }
    }

    fn print(&self) {
        println!(
            "BasicInterval: Range: [{}, {}], Data: {:?}",
            self.range.get_lower(),
            self.range.get_upper(),
            self.data
        );
    }
}

#[derive(Debug)]
pub struct SymbInterval<'tcx, T> {
    base: BasicInterval<'tcx, T>,
    bound: &'tcx Place<'tcx>,
    predicate: Predicate,
}

impl<'tcx, T> SymbInterval<'tcx, T> {
    pub fn new(range: Range, bound: &Place, predicate: Predicate) -> Self {
        Self {
            base: BasicInterval::new(range),
            bound,
            predicate,
        }
    }

    pub fn get_operation(&self) -> &Predicate {
        &self.predicate
    }

    pub fn get_bound(&self) -> &Place {
        &self.bound
    }

    pub fn fix_intersects(&self, bound: &Place, sink: &Place) -> Range {
        println!(
            "Fixing intersects with bound {:?} and sink {:?}",
            bound, sink
        );
    }
}

impl<'tcx, T> BasicIntervalTrait<'tcx, T> for SymbInterval<'tcx, T> {
    // fn get_value_id(&self) -> IntervalId {
    //     IntervalId::SymbIntervalId
    // }

    fn get_range(&self) -> &Range {
        self.base.get_range()
    }

    fn set_range(&mut self, new_range: Range) {
        self.base.set_range(new_range);
    }

    fn print(&self) {
        println!(
            "SymbInterval: Range: [{}, {}], Data: {:?}, Bound: {:?}, Predicate: {:?}",
            self.get_range().get_lower(),
            self.get_range().get_upper(),
            self.base.get_data(),
            self.bound,
            self.predicate
        );
    }
}

// Define the basic operation trait
pub trait Operation {
    fn get_value_id(&self) -> u32; // Placeholder for an operation identifier
    fn eval(&self) -> Range; // Method to evaluate the result of the operation
    fn print(&self, os: &mut dyn fmt::Write);
}

// Define the BasicOp struct
pub struct BasicOp<'tcx, T> {
    pub intersect: &'tcx BasicInterval<'tcx, T>, // The range associated with the operation
    pub sink: &'tcx VarNode<'tcx, T>,            // The target node storing the result
    pub inst: Option<Instruction>,               // The instruction that originated this operation
}

impl<'tcx, T> BasicOp<'tcx, T> {
    // Constructor for creating a new BasicOp
    pub fn new(
        intersect: Option<BasicInterval>,
        sink: &VarNode,
        inst: Option<Instruction>,
    ) -> Self {
        BasicOp {
            intersect,
            sink,
            inst,
        }
    }

    // Returns the instruction that originated this operation
    pub fn get_instruction(&self) -> Option<&Instruction> {
        self.inst.as_ref()
    }

    // Replaces symbolic intervals with constants (this could involve more logic)
    pub fn fix_intersects(&mut self, v: &VarNode) {
        // Logic for replacing symbolic intervals with constants
    }

    // Sets a new range for the operation
    pub fn set_intersect(&mut self, new_intersect: Range) {
        if let Some(ref mut interval) = self.intersect {
            interval.set_range(new_intersect);
        }
    }

    // Returns the target of the operation (sink)
    pub fn get_sink(&self) -> &VarNode {
        &self.sink
    }

    // Returns the target of the operation (sink), mutable version
    pub fn get_sink_mut(&mut self) -> &mut VarNode {
        &mut self.sink
    }
}

// Implement the Operation trait for BasicOp
impl<'tcx> Operation for BasicOp<'tcx> {
    fn get_value_id(&self) -> u32 {
        0 // Placeholder implementation
    }

    fn eval(&self) -> Range {
        // Placeholder for evaluating the range
    }

    fn print(&self, os: &mut dyn fmt::Write) {
        write!(os, "BasicOp with sink: {}", self.sink.name).unwrap();
    }
}
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct VarNode<'tcx, T> {
    // The program variable which is represented.
    v: &'tcx Place<'tcx>,
    // A Range associated to the variable.
    interval: Range<T>,
    // Used by the crop meet operator.
    abstract_state: char,
}
impl<'tcx, T> VarNode<'tcx, T> {
    pub fn new(v: &Place) -> Self {
        Self {
            v,
            interval: Range::default(),
            abstract_state: '?',
        }
    }

    /// Initializes the value of the node.
    pub fn init(&mut self, outside: bool) {
        let value = self.get_value();

        // if let Some(ci) = value.as_constant_int() {
        //     let tmp = ci.get_value();
        //     if tmp.bits() < MAX_BIT_INT {
        //         self.set_range(Range::new(
        //             tmp.extend_bits(MAX_BIT_INT),
        //             tmp.extend_bits(MAX_BIT_INT),
        //         ));
        //     } else {
        //         self.set_range(Range::new(tmp, tmp));
        //     }
        // } else {
        //     if !outside {
        //         self.set_range(Range::new(MIN, MAX));
        //     } else {
        //         self.set_range(Range::new(MIN, MAX));
        //     }
        // }
    }

    /// Returns the range of the variable represented by this node.
    pub fn get_range(&self) -> &Range {
        &self.interval
    }

    /// Returns the variable represented by this node.
    pub fn get_value(&self) -> &Place {
        &self.v
    }

    /// Changes the status of the variable represented by this node.
    pub fn set_range(&mut self, new_interval: Range) {
        self.interval = new_interval;

        // Check if lower bound is greater than upper bound. If it is,
        // set range to empty.
        if self.interval.get_lower().sgt(self.interval.get_upper()) {
            self.interval.set_empty();
        }
    }

    /// Pretty print.
    pub fn print(&self, os: &mut dyn std::io::Write) {
        // Implementation of pretty printing using the `os` writer.
    }

    pub fn get_abstract_state(&self) -> char {
        self.abstract_state
    }

    /// The possible states are '0', '+', '-', and '?'.
    pub fn store_abstract_state(&mut self) {
        // Implementation of storing the abstract state.
    }
}
pub struct ValueBranchMap<'tcx, T> {
    v: &'tcx Place<'tcx>,                // The value associated with the branch
    bb_true: &'tcx BasicBlock<'tcx>,     // True side of the branch
    bb_false: &'tcx BasicBlock<'tcx>,    // False side of the branch
    itv_t: &'tcx BasicInterval<'tcx, T>, // Interval for the true side
    itv_f: &'tcx BasicInterval<'tcx, T>,
}
impl<'tcx, T> ValueBranchMap<'tcx, T> {
    pub fn new(
        v: &'tcx Place<'tcx>,
        bb_true: &'tcx BasicBlock<'tcx>,
        bb_false: &'tcx BasicBlock<'tcx>,
        itv_t: &'tcx BasicInterval<'tcx, T>,
        itv_f: &'tcx BasicInterval<'tcx, T>,
    ) -> Self {
        Self {
            v,
            bb_true,
            bb_false,
            itv_t,
            itv_f,
        }
    }

    /// Get the "false side" of the branch
    pub fn get_bb_false(&self) -> &BasicBlock<'tcx> {
        self.bb_false
    }

    /// Get the "true side" of the branch
    pub fn get_bb_true(&self) -> &BasicBlock<'tcx> {
        self.bb_true
    }

    /// Get the interval associated with the true side of the branch
    pub fn get_itv_t(&self) -> &BasicInterval<'tcx> {
        self.itv_t
    }

    /// Get the interval associated with the false side of the branch
    pub fn get_itv_f(&self) -> &BasicInterval<'tcx> {
        self.itv_f
    }

    /// Get the value associated with the branch
    pub fn get_v(&self) -> &Place<'tcx> {
        self.v
    }

    /// Change the interval associated with the true side of the branch
    pub fn set_itv_t(&mut self, itv: &BasicInterval) {
        self.itv_t = itv;
    }

    /// Change the interval associated with the false side of the branch
    pub fn set_itv_f(&mut self, itv: &BasicInterval) {
        self.itv_f = itv;
    }

    // pub fn clear(&mut self) {
    //     self.itv_t = Box::new(EmptyInterval::new());
    //     self.itv_f = Box::new(EmptyInterval::new());
    // }
}
pub type VarNodes<'tcx, T> = HashMap<&'tcx Place<'tcx>, &'tcx VarNode<'tcx, T>>;
pub type GenOprs<'tcx, T> = HashSet<&'tcx BasicOp<'tcx, T>>;
pub type UseMap<'tcx, T> = HashMap<&'tcx Place<'tcx>, HashSet<&'tcx BasicOp<'tcx, T>>>;
pub type SymbMap<'tcx, T> = HashMap<&'tcx Place<'tcx>, HashSet<&'tcx BasicOp<'tcx, T>>>;
pub type DefMap<'tcx, T> = HashMap<&'tcx Place<'tcx>, &'tcx BasicOp<'tcx, T>>;
pub type ValuesBranchMap<'tcx, T> = HashMap<&'tcx Place<'tcx>, ValueBranchMap<'tcx, T>>;
// pub type ValuesSwitchMap<'tcx, T> = HashMap<&'tcx Place<'tcx>, ValueSwitchMap<'tcx, T>>;
