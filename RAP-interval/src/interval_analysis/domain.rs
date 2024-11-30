use rustc_middle::mir::LocalDecl;

use crate::interval_analysis::range::Range;
pub struct VarNode<'tcx> {
    // The program variable which is represented.
    v: &'tcx LocalDecl,
    // A Range associated to the variable.
    interval: Range<i32>,
    // Used by the crop meet operator.
    abstract_state: char,
}

impl<'tcx> VarNode<'tcx> {
    pub fn new(v: &LocalDecl) -> Self {
        Self {
            v: &LocalDecl,
            interval: Range::default(),
            abstract_state: '?',
        }
    }

    /// Initializes the value of the node.
    pub fn init(&mut self, outside: bool) {
        let value = self.get_value();

        if let Some(ci) = value.as_constant_int() {
            // 使用 rustc 中的相关 MIR 类型处理常量
            let tmp = ci.get_value();
            if tmp.bits() < MAX_BIT_INT {
                // 扩展到指定的位宽
                self.set_range(Range::new(
                    tmp.extend_bits(MAX_BIT_INT),
                    tmp.extend_bits(MAX_BIT_INT),
                ));
            } else {
                self.set_range(Range::new(tmp, tmp));
            }
        } else {
            if !outside {
                // 初始化为一个基本的、未知的区间
                self.set_range(Range::new(MIN, MAX)); // 需要根据实际情况定义 Range 结构
            } else {
                self.set_range(Range::new(MIN, MAX)); // 基本区间
            }
        }
    }

    /// Returns the range of the variable represented by this node.
    pub fn get_range(&self) -> &Range {
        &self.interval
    }

    /// Returns the variable represented by this node.
    pub fn get_value(&self) -> &LocalDecl {
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
