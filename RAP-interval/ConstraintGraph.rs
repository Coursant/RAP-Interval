use super::{domain::*, range::RangeType, range::*};

use rustc_middle::{mir::*, ty::*};
use rustc_mir_transform::*;
use std::collections::{HashMap, HashSet};
pub struct ConstraintGraph<'tcx, T> {
    // Protected fields
    vars: VarNodes<'tcx, T>, // The variables of the source program
    oprs: GenOprs<'tcx, T>,  // The operations of the source program

    // Private fields
    // func: Option<Function>,             // Save the last Function analyzed
    defmap: DefMap<'tcx, T>, // Map from variables to the operations that define them
    usemap: UseMap<'tcx, T>, // Map from variables to operations where variables are used
    symbmap: SymbMap<'tcx, T>, // Map from variables to operations where they appear as bounds
    values_branchmap: ValuesBranchMap<'tcx, T>, // Store intervals, basic blocks, and branches
    values_switchmap: ValuesSwitchMap<'tcx, T>, // Store intervals for switch branches
    constant_vector: Vec<APInt>, // Vector for constants from an SCC
}

impl<'tcx, T> ConstraintGraph<'tcx, T> {
    pub fn new() -> Self {
        Self {
            vars: VarNodes::new(),
            oprs: GenOprs::new(),
            // func: None,
            defmap: DefMap::new(),
            usemap: UseMap::new(),
            symbmap: SymbMap::new(),
            values_branchmap: ValuesBranchMap::new(),
            values_switchmap: ValuesSwitchMap::new(),
            constant_vector: Vec::new(),
        }
    }

    pub fn add_varnode(&mut self, v: &Place) -> VarNode {
        // Adds a VarNode to the graph
        let node = VarNode::new(v);
        self.vars.insert(v, &node);
        let uselist: HashSet<BasicOp> = HashSet::new();
        self.usemap.insert(v, &uselist)
    }

    pub fn get_oprs(&self) -> &GenOprs {
        &self.oprs
    }

    pub fn get_defmap(&self) -> &DefMap {
        &self.defmap
    }

    pub fn get_usemap(&self) -> &UseMap {
        &self.usemap
    }

    // pub fn build_graph(&self, body: &Body) -> ConstraintGraph {
    //     let mut graph = ConstraintGraph::new();
    //     let basic_blocks = &body.basic_blocks;
    //     for basic_block_data in basic_blocks.iter() {
    //         for statement in basic_block_data.statements.iter() {
    //             graph.add_stat_to_graph(&statement.kind);
    //         }
    //         if let Some(terminator) = &basic_block_data.terminator {
    //             graph.add_terminator_to_graph(&terminator.kind);
    //         }
    //     }
    //     graph
    // }
    pub fn build_graph(&mut self, body: &'tcx Body) {
        self.build_value_maps(body, self.tcx);
        for block in body.basic_blocks() {
            let block_data = &body[block];
            // Traverse statements
            for statement in block_data.statements.iter() {
                self.build_operations(statement);
            }
        }
    }
    pub fn build_value_maps(&self, body: &Body<'tcx>, tcx: TyCtxt<'tcx>) {
        for (block_index, block) in body.basic_blocks.iter_enumerated() {
            if let Some(terminator) = &block.terminator {
                match &terminator.kind {
                    TerminatorKind::SwitchInt { discr, targets } => {
                        self.build_value_branch_map(tcx, body, discr, targets, block);
                    }
                    TerminatorKind::Goto { target } => {
                        self.build_value_goto_map(block_index, *target);
                    }
                    _ => {
                        // println!(
                        //     "BasicBlock {:?} has an unsupported terminator: {:?}",
                        //     block_index, terminator.kind
                        // );
                    }
                }
            }
        }
    }

    pub fn build_value_branch_map(
        &mut self,
        tcx: TyCtxt<'tcx>,
        body: &Body<'tcx>,
        discr: Operand<'tcx>,
        targets: SwitchTargets,
        block: &BasicBlockData<'tcx>,
    ) {
        // 确保分支条件是二元比较
        if let Operand::Copy(place) | Operand::Move(place) = discr {
            if let Some((op1, op2, cmp_op)) = self.extract_condition(&place, block) {
                // 获取分支目标
                self.add_varnode(op1.place());
                self.add_varnode(op2.place());
                let true_block = targets.target_for_value(0);
                let false_block = targets.target_for_value(0);

                let (true_range, false_range) = self.calculate_ranges(op1, op2, cmp_op, tcx);

                let vbm = ValueBranchMap::new(
                    &place,
                    &true_block,
                    &false_block,
                    &BasicInterval::new(true_range),
                    &BasicInterval::new(false_range),
                );
                self.values_branch_map.insert(place, vbm);
            };
        }
    }

    fn extract_condition(
        &self,
        place: &Place<'tcx>,
        block: &BasicBlockData<'tcx>,
    ) -> Option<(Operand<'tcx>, Operand<'tcx>, BinOp)> {
        for stmt in &block.statements {
            if let StatementKind::Assign(box (lhs, Rvalue::BinaryOp(bin_op, box (op1, op2)))) =
                &stmt.kind
            {
                if lhs == place {
                    return Some((*op1, *op2, *bin_op));
                }
            }
        }
        None
    }
    pub fn calculate_ranges(
        &self,
        op1: Operand<'tcx>,
        op2: Operand<'tcx>,
        cmp_op: BinOp,
        tcx: TyCtxt<'tcx>,
    ) -> (Option<(i128, i128)>, Option<(i128, i128)>) {
        // 检查操作数是否为常量
        let const_op1 = op1.constant();
        let const_op2 = op2.constant();

        match (const_op1, const_op2) {
            (Some(c1), Some(c2)) => {}
            (Some(c), None) | (None, Some(c)) => {
                let const_in_left: bool;
                if const_op1.is_some() {
                    const_in_left = true;
                } else {
                    const_in_left = false;
                }
                // 此处应根据T进行选取，设定为scalarInt
                let const_range = Range::new(c.const_.try_to_scalar().unwrap());

                let true_range = self.apply_comparison(const_range, cmp_op, true, const_in_left);
                let false_range = self.apply_comparison(const_range, cmp_op, false, const_in_left);

                (true_range, false_range)
            }
            (None, None) => {
                // 两个变量之间的比较
                let variable_range1 = Range::new(UserType::new());
                let variable_range2 = Range::new(UserType::new());
                let true_range =
                    self.apply_comparison(variable_range1, variable_range2, cmp_op, true);
                let false_range =
                    self.apply_comparison(variable_range1, variable_range2, cmp_op, false);
                (true_range, false_range)
            }
        }
    }

    /// 从操作数中提取常量值

    /// 根据比较条件评估真/假分支的范围
    fn apply_comparison(
        &self,
        const_range: Range<T>,
        cmp_op: BinOp,
        is_true_branch: bool,
        const_in_left: bool,
    ) -> Option<(T, T)> {
        match cmp_op {
            BinOp::Lt => {
                if is_true_branch ^ const_in_left {
                    Range::with_bounds(T::min, const_range.get_lower(), RangeType::Regular)
                } else {
                    Range::with_bounds(const_range.get_upper(), T::max, RangeType::Regular)
                }
            }

            BinOp::Le => {
                if is_true_branch ^ const_in_left {
                    Range::with_bounds(T::min, const_range.get_lower(), RangeType::Regular)
                } else {
                    Range::with_bounds(const_range.get_upper(), T::max, RangeType::Regular)
                }
            }

            BinOp::Gt => {
                if is_true_branch ^ const_in_left {
                    Range::with_bounds(T::min, const_range.get_lower(), RangeType::Regular)
                } else {
                    Range::with_bounds(const_range.get_upper(), T::max, RangeType::Regular)
                }
            }

            BinOp::Ge => {
                if is_true_branch ^ const_in_left {
                    Range::with_bounds(T::min, const_range.get_lower(), RangeType::Regular)
                } else {
                    Range::with_bounds(const_range.get_upper(), T::max, RangeType::Regular)
                }
            }

            BinOp::Eq => {
                if is_true_branch ^ const_in_left {
                    Range::with_bounds(T::min, const_range.get_lower(), RangeType::Regular)
                } else {
                    Range::with_bounds(const_range.get_upper(), T::max, RangeType::Regular)
                }
            }

            _ => Range::empty(),
        }
    }

    fn build_value_goto_map(&self, block_index: BasicBlock, target: BasicBlock) {
        println!(
            "Building value map for Goto in block {:?} targeting block {:?}",
            block_index, target
        );
        // 在这里实现具体的 Goto 处理逻辑
    }
    pub fn build_varnodes(&mut self) {
        // Builds VarNodes
        for (name, node) in self.vars.iter_mut() {
            let is_undefined = !self.defmap.contains_key(name);
            node.init(is_undefined);
        }
    }
    pub fn build_operations(&mut self, inst: &Statement) {
        // Handle binary instructions
        if let StatementKind::Assign(box (place, rvalue)) = &inst.kind {
            match rvalue {
                Rvalue::BinaryOp(op, box (op1, op2)) => {
                    self.add_varnode(op1.place());
                    self.add_varnode(op2.place());
                    self.add_varnode(place);
                    self.add_binary_op(inst);
                }
                _ => {}
            }
        }
        match inst.kind {
            StatementKind::BinaryOp(_) => {
                self.add_binary_op(inst);
            }
            StatementKind::PHI(phi) => {
                if phi.name().starts_with(sigma_string) {
                    self.add_sigma_op(phi);
                } else {
                    self.add_phi_op(phi);
                }
            }
            StatementKind::UnaryOp(_) => {
                self.add_unary_op(inst);
            }
            _ => {
                // Handle other cases if necessary
            }
        }
    }
    fn add_unary_op(&mut self, inst: &Statement) {
        // Implementation for adding unary operation
        // ...
    }

    fn add_binary_op(&mut self, inst: &Statement) {
        // Implementation for adding binary operation
        // ...
    }

    fn add_phi_op(&mut self, phi: &'tcx PHINode<'tcx>) {
        // Implementation for adding phi operation
        // ...
    }

    fn add_sigma_op(&mut self, phi: &'tcx PHINode<'tcx>) {
        // Implementation for adding sigma operation
        // ...
    }
    pub fn find_intervals(&mut self) {
        // 构建符号交集映射
        self.build_symbolic_intersect_map();

        // 查找强连通分量（SCC）
        let scc_list = Nuutila::new(&self.vars, &self.usemap, &self.symbmap);
        self.num_sccs += scc_list.worklist.len();

        // 遍历每个 SCC
        for component in scc_list.components() {
            if component.len() == 1 {
                // 处理单节点的 SCC
                self.num_alone_sccs += 1;
                self.fix_intersects(&component);

                let var = component.iter().next().unwrap();
                if var.get_range().is_unknown() {
                    var.set_range(Range {
                        min: i32::MIN,
                        max: i32::MAX,
                    });
                }
            } else {
                // 更新最大 SCC 大小
                if component.len() > self.size_max_scc {
                    self.size_max_scc = component.len();
                }

                // 为该 SCC 构建使用映射
                let comp_use_map = self.build_use_map(&component);

                // 获取 SCC 的入口点
                let mut entry_points = HashSet::new();
                self.generate_entry_points(&component, &mut entry_points);

                // 固定点迭代，更新范围
                self.pre_update(&comp_use_map, &entry_points);
                self.fix_intersects(&component);

                // 为未知范围的变量设置默认范围
                for var in &component {
                    if var.get_range().is_unknown() {
                        var.set_range(Range {
                            min: i32::MIN,
                            max: i32::MAX,
                        });
                    }
                }

                // 二次迭代，更新活动变量
                let mut active_vars = HashSet::new();
                self.generate_active_vars(&component, &mut active_vars);
                self.pos_update(&comp_use_map, &active_vars, &component);
            }

            // 将结果传播到下一个 SCC
            self.propagate_to_next_scc(&component);
        }
    }

    // 假设的辅助方法定义
    fn build_symbolic_intersect_map(&self) {
        // 构建符号交集映射
    }
}

pub struct Nuutila<'a> {
    worklist: Vec<&'a Rc<VarNode>>,
    components: Vec<HashSet<Rc<VarNode>>>,
}

impl<'a> Nuutila<'a> {
    fn new(
        vars: &'a VarNodes,
        use_map: &'a HashMap<String, Vec<Rc<VarNode>>>,
        symb_map: &'a HashMap<String, Vec<Rc<VarNode>>>,
    ) -> Self {
        Nuutila {
            worklist: Vec::new(),
            components: Vec::new(),
        }
    }

    fn components(&self) -> &Vec<HashSet<Rc<VarNode>>> {
        &self.components
    }
}
