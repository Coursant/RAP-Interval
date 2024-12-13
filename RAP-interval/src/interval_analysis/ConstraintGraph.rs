use super::{domain::*, range::Range};

use rustc_middle::{mir::*, ty::*};
use std::collections::{HashMap, HashSet};

pub struct ConstraintGraph<'tcx> {
    // Protected fields
    vars: VarNodes<'tcx>, // The variables of the source program
    oprs: GenOprs<'tcx>,  // The operations of the source program

    // Private fields
    // func: Option<Function>,             // Save the last Function analyzed
    defmap: DefMap,   // Map from variables to the operations that define them
    usemap: UseMap,   // Map from variables to operations where variables are used
    symbmap: SymbMap, // Map from variables to operations where they appear as bounds
    values_branchmap: ValuesBranchMap, // Store intervals, basic blocks, and branches
    values_switchmap: ValuesSwitchMap, // Store intervals for switch branches
    constant_vector: Vec<APInt>, // Vector for constants from an SCC
}

impl ConstraintGraph {
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

    pub fn add_unary_op(&mut self, i: &Instruction) {
        // Adds an UnaryOp to the graph
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
    pub fn build_graph<'tcx>(&mut self, body: &'tcx Body) {
        self.build_value_maps(body, self.tcx);

        for inst in func.instructions() {
            let ty = inst.get_type();

            if !ty.is_integer() {
                continue;
            }

            if !self.is_valid_instruction(&inst) {
                continue;
            }

            self.build_operations();
        }
    }

    pub fn build_value_maps<'tcx>(&self, body: &Body<'tcx>, tcx: TyCtxt<'tcx>) {
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

    pub fn build_value_branch_map<'tcx>(
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
                let true_block = targets[0];
                let false_block = targets[1];

                // 分析操作数和条件，计算范围
                let (true_range, false_range) = self.calculate_ranges(op1, op2, cmp_op, tcx);

                if let Some(local) = place.as_local() {
                    let vbm = ValueBranchMap {
                        variable: local,
                        true_block,
                        false_block,
                        true_range,
                        false_range,
                    };
                    self.values_branch_map.insert(local, vbm);
                }
            }
        }
    }

    fn extract_condition<'tcx>(
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
    pub fn calculate_ranges<'tcx>(
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
                // 单变量与常量比较
                self.add_varnode(c.place());
                let variable_range = Range::new(UserType::new()); //使用userType
                let true_range = self.apply_comparison(c, variable_range, cmp_op, true);
                let false_range = self.apply_comparison(c, variable_range, cmp_op, false);
                (true_range, false_range)
            }
            (None, None) => {
                // 两个变量之间的比较
                self.add_varnode(op1.place());
                self.add_varnode(op2.place());
                let variable_range1 = Range::new(UserType::new()); //使用userType
                let variable_range2 = Range::new(UserType::new()); //使用userType
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
    fn apply_comparison<T>(
        &self,
        constant: i128,
        variable_range: Range<T>,
        cmp_op: BinOp,
        is_true_branch: bool,
    ) -> Option<(i128, i128)> {
        match cmp_op {
            BinOp::Lt if is_true_branch => Some((variable_range.0, constant - 1)),
            BinOp::Lt if !is_true_branch => Some((constant, variable_range.1)),
            BinOp::Le if is_true_branch => Some((variable_range.0, constant)),
            BinOp::Le if !is_true_branch => Some((constant + 1, variable_range.1)),
            BinOp::Gt if is_true_branch => Some((constant + 1, variable_range.1)),
            BinOp::Gt if !is_true_branch => Some((variable_range.0, constant)),
            BinOp::Ge if is_true_branch => Some((constant, variable_range.1)),
            BinOp::Ge if !is_true_branch => Some((variable_range.0, constant - 1)),
            BinOp::Eq if is_true_branch => Some((constant, constant)),
            BinOp::Eq if !is_true_branch => None, // 不相等的范围较难确定
            _ => None,
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
    }

    pub fn build_symbolic_intersect_map(&mut self) {
        // Builds symbolic intersect map
    }

    pub fn build_usemap(&self, component: &SmallPtrSet<VarNode, 32>) -> UseMap {
        // Builds the use map for a component
    }

    pub fn propagate_to_next_scc(&mut self, component: &SmallPtrSet<VarNode, 32>) {
        // Propagates data to the next SCC
    }

    pub fn find_intervals(&mut self) {
        // Finds intervals of the variables in the graph
    }

    pub fn generate_entry_points(
        &self,
        component: &SmallPtrSet<VarNode, 32>,
        entry_points: &mut SmallPtrSet<Value, 6>,
    ) {
        // Generates entry points
    }

    pub fn fix_intersects(&mut self, component: &SmallPtrSet<VarNode, 32>) {
        // Fixes intersections
    }

    pub fn generate_active_vars(
        &self,
        component: &SmallPtrSet<VarNode, 32>,
        active_vars: &mut SmallPtrSet<Value, 6>,
    ) {
        // Generates active variables
    }

    pub fn clear(&mut self) {
        // Releases memory used by the graph
    }

    pub fn print(&self, f: &Function, os: &mut raw_ostream) {
        // Prints the graph in dot format
    }

    pub fn print_to_file(&self, f: &Function, file_name: &str) {
        // Prints graph to a file
    }

    pub fn dump(&self, f: &Function) {
        self.print(f, &mut dbgs());
        dbgs().write("\n");
    }

    pub fn print_result_intervals(&self) {
        // Prints result intervals
    }

    pub fn compute_stats(&self) {
        // Computes stats
    }

    pub fn get_range(&self, v: &Value) -> Range {
        // Gets range for a value
    }
    fn find_intervals(&mut self) {
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

    fn fix_intersects(&self, component: &HashSet<Rc<VarNode>>) {
        // 修复交集
    }

    fn build_use_map(&self, component: &HashSet<Rc<VarNode>>) -> HashMap<String, Vec<Rc<VarNode>>> {
        // 构建使用映射
        HashMap::new()
    }

    fn generate_entry_points(
        &self,
        component: &HashSet<Rc<VarNode>>,
        entry_points: &mut HashSet<String>,
    ) {
        // 生成入口点
    }

    fn pre_update(
        &self,
        comp_use_map: &HashMap<String, Vec<Rc<VarNode>>>,
        entry_points: &HashSet<String>,
    ) {
        // 预更新范围
    }

    fn generate_active_vars(
        &self,
        component: &HashSet<Rc<VarNode>>,
        active_vars: &mut HashSet<String>,
    ) {
        // 生成活动变量
    }

    fn pos_update(
        &self,
        comp_use_map: &HashMap<String, Vec<Rc<VarNode>>>,
        active_vars: &HashSet<String>,
        component: &HashSet<Rc<VarNode>>,
    ) {
        // 二次更新范围
    }

    fn propagate_to_next_scc(&self, component: &HashSet<Rc<VarNode>>) {
        // 将结果传播到下一个 SCC
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
