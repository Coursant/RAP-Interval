use rustc_data_structures::graph::dominators::Dominators;
// use rustc_mir_transform::ssa::SsaLocals;
// use crate::ssa::SsaLocals;
use rustc_data_structures::graph::{dominators, Predecessors};
use rustc_hir::def_id::LocalDefId;
use rustc_index::{Idx, IndexVec};
use rustc_interface::{interface::Compiler, Queries};
use rustc_middle::mir::pretty::*;
use rustc_middle::mir::*;
use rustc_middle::{
    mir::{visit::Visitor, Body, Local, Location},
    ty::TyCtxt,
};
use rustc_span::sym::new;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::rc::Rc;
use tracing::{debug, error, info, warn};

// impl<'tcx> SSAContext {}

// pub struct SSAtransform<'tcx> {
//     /// 保存每个变量的当前版本
//     version_map: IndexVec<Local, usize>,
//     /// 保存每个基本块的前驱块信息
//     phi_inserts: IndexVec<BasicBlock, Vec<(Local, Vec<Operand<'tcx>>)>>,
// }

// impl<'tcx> SSAtransform<'tcx> {
//     /// 创建一个新的 `SSAtransform` 实例
//     pub fn new(body: &Body<'tcx>) -> Self {
//         Self {
//             version_map: IndexVec::from_elem(0, &body.local_decls),
//             phi_inserts: IndexVec::from_elem(Vec::new(), &body.basic_blocks),
//         }
//     }

//     /// 执行 SSA 转换
//     pub fn apply(&mut self, tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
//         self.collect_versions_and_phi_places(body);
//         self.insert_phi_functions(body);
//     }

//     /// 收集变量版本和需要插入 Phi 函数的位置
//     fn collect_versions_and_phi_places(&mut self, body: &Body<'tcx>) {
//         for (bb_idx, bb) in body.basic_blocks.iter_enumerated() {
//             for stmt in &bb.statements {
//                 if let StatementKind::Assign(box (place, _)) = &stmt.kind {
//                     if let Some(local) = place.as_local() {
//                         // 更新变量版本
//                         self.version_map[local] += 1;
//                     }
//                 }
//             }

//             // if let Some(terminator) = &bb.terminator {
//             //     for &target in terminator.successors() {
//             //         for local in self.version_map.indices() {
//             //             // 收集需要在目标块插入的变量值
//             //             let current_operand = Operand::Copy(
//             //                 // Place::from(local).with_field(None, self.version_map[local]),
//             //             );
//             //             self.phi_inserts[target].push((local, vec![current_operand]));
//             //         }
//             //     }
//             // }
//         }
//     }

//     /// 插入 Phi 函数到每个目标块的头部

// }

pub struct SSATransformer<'tcx> {
    tcx: TyCtxt<'tcx>, // TyCtxt 上下文
    def_id: LocalDefId,
    pub body: Rc<RefCell<Body<'tcx>>>,
    // pub body:     &'tcx  Body<'tcx>,                  // MIR 的优化中间表示
    cfg: HashMap<BasicBlock, Vec<BasicBlock>>, // 控制流图
    dominators: Dominators<BasicBlock>,        // 支配者分析结果
    dom_tree: HashMap<BasicBlock, Vec<BasicBlock>>, // 支配树
    df: HashMap<BasicBlock, HashSet<BasicBlock>>, // 支配前沿
    local_assign_blocks: HashMap<Local, HashSet<BasicBlock>>, // 局部变量的赋值块映射
    reaching_def: HashMap<Local, Option<Local>>,
    local_defination_block: HashMap<Local, BasicBlock>,
}

impl<'tcx> SSATransformer<'tcx> {
    pub fn new(tcx: TyCtxt<'tcx>, def_id: LocalDefId) -> Self {
        let mut body_clone = tcx.optimized_mir(def_id).clone();
        let body_ref = Rc::new(RefCell::new(body_clone));
        let cfg: HashMap<BasicBlock, Vec<BasicBlock>> =
            Self::extract_cfg_from_predecessors(&body_ref.borrow());

        let dominators: Dominators<BasicBlock> =
            body_ref.borrow().basic_blocks.dominators().clone();

        let dom_tree: HashMap<BasicBlock, Vec<BasicBlock>> =
            Self::construct_dominance_tree(&body_ref.borrow());

        let df: HashMap<BasicBlock, HashSet<BasicBlock>> =
            Self::compute_dominance_frontier(&body_ref.borrow(), &dom_tree);

        let local_assign_blocks: HashMap<Local, HashSet<BasicBlock>> =
            Self::map_locals_to_assign_blocks(&body_ref.borrow());
        let local_defination_block: HashMap<Local, BasicBlock> =
            Self::map_locals_to_definition_block(&body_ref.borrow());
        SSATransformer {
            tcx,
            def_id,
            // body:RefCell::new(body),
            body: body_ref,
            cfg,
            dominators,
            dom_tree,
            df,
            local_assign_blocks,
            reaching_def: HashMap::default(),
            local_defination_block,
        }
    }

    /// 打印分析结果
    ///
    pub fn print_phi_mir(&self) {
        let dir_path = "ssa_mir";
        let phi_mir_file_path = format!("{}/phi_mir_{:?}.txt", dir_path, self.def_id);
        let mut file = File::create(&phi_mir_file_path).unwrap();
        let mut w = io::BufWriter::new(&mut file);
        let options = PrettyPrintMirOptions::from_cli(self.tcx);
        write_mir_fn(
            self.tcx,
            &self.body.borrow(),
            &mut |_, _| Ok(()),
            &mut w,
            options,
        )
        .unwrap();
    }
    pub fn analyze(&self) {
        // println!("{:?}", self.cfg);
        // println!("{:?}", self.dominators);
        // println!("!!!!!!!!!!!!!!!!!!!!!!!!");
        print!("\ndom_tree{:?}", self.dom_tree);
        Self::print_dominance_tree(&self.dom_tree, START_BLOCK, 0);
        // print!("{:?}", self.df);
        // println!("!!!!!!!!!!!!!!!!!!!!!!!!");
        // print!("{:?}", self.local_assign_blocks);
        // print!("\n local_defination_block before phi {:?}", self.local_defination_block);
        let dir_path = "ssa_mir";

        // 动态生成文件路径
        let mir_file_path = format!("{}/mir_{:?}.txt", dir_path, self.def_id);
        let phi_mir_file_path = format!("{}/ssa_mir_{:?}.txt", dir_path, self.def_id);
        let mut file = File::create(&mir_file_path).unwrap();
        let mut w1 = io::BufWriter::new(&mut file);
        write_mir_pretty(self.tcx, None, &mut w1).unwrap();
        let mut file2 = File::create(&phi_mir_file_path).unwrap();
        let mut w2 = io::BufWriter::new(&mut file2);
        let options = PrettyPrintMirOptions::from_cli(self.tcx);
        write_mir_fn(
            self.tcx,
            &self.body.borrow(),
            &mut |_, _| Ok(()),
            &mut w2,
            options,
        )
        .unwrap();
    }
    fn depth_first_search_postorder(
        dom_tree: &HashMap<BasicBlock, Vec<BasicBlock>>,
    ) -> Vec<BasicBlock> {
        let mut visited: HashSet<BasicBlock> = HashSet::new();
        let mut postorder = Vec::new();

        fn dfs(
            node: BasicBlock,
            dom_tree: &HashMap<BasicBlock, Vec<BasicBlock>>,
            visited: &mut HashSet<BasicBlock>,
            postorder: &mut Vec<BasicBlock>,
        ) {
            if visited.insert(node) {
                // 遍历当前节点的子节点
                if let Some(children) = dom_tree.get(&node) {
                    for &child in children {
                        dfs(child, dom_tree, visited, postorder);
                    }
                }
                // 当前节点访问结束，加入后序结果
                postorder.push(node);
            }
        }

        // 开始从支配树的任意一个根节点进行 DFS
        if let Some(&start_node) = dom_tree.keys().next() {
            dfs(start_node, dom_tree, &mut visited, &mut postorder);
        }

        postorder
    }
    fn map_locals_to_definition_block(body: &Body) -> HashMap<Local, BasicBlock> {
        let mut local_to_block_map: HashMap<Local, BasicBlock> = HashMap::new();

        // 遍历每个基本块
        for (bb, block_data) in body.basic_blocks.iter_enumerated() {
            // 遍历当前基本块中的每条语句
            for statement in &block_data.statements {
                match &statement.kind {
                    // 如果语句是一个赋值语句
                    StatementKind::Assign(box (place, _)) => {
                        // 如果是局部变量（local）的定义
                        if let Some(local) = place.as_local() {
                            // 只有第一次遇到局部变量时才会映射它
                            local_to_block_map.entry(local).or_insert(bb);
                        }
                    }
                    _ => {}
                }
            }
        }

        local_to_block_map
    }
    fn map_locals_to_assign_blocks(body: &Body) -> HashMap<Local, HashSet<BasicBlock>> {
        let mut local_to_blocks: HashMap<Local, HashSet<BasicBlock>> = HashMap::new();

        for (bb, data) in body.basic_blocks.iter_enumerated() {
            for stmt in &data.statements {
                if let StatementKind::Assign(box (place, _)) = &stmt.kind {
                    let local = place.local;

                    // 获取或初始化 HashSet
                    local_to_blocks
                        .entry(local)
                        .or_insert_with(HashSet::new)
                        .insert(bb);
                }
            }
        }

        local_to_blocks
    }
    fn construct_dominance_tree(body: &Body<'_>) -> HashMap<BasicBlock, Vec<BasicBlock>> {
        let mut dom_tree: HashMap<BasicBlock, Vec<BasicBlock>> = HashMap::new();
        let dominators = body.basic_blocks.dominators();
        for (block, _) in body.basic_blocks.iter_enumerated() {
            if let Some(idom) = dominators.immediate_dominator(block) {
                dom_tree.entry(idom).or_default().push(block);
            }
        }

        dom_tree
    }
    fn compute_dominance_frontier(
        body: &Body<'_>,
        dom_tree: &HashMap<BasicBlock, Vec<BasicBlock>>,
    ) -> HashMap<BasicBlock, HashSet<BasicBlock>> {
        let mut dominance_frontier: HashMap<BasicBlock, HashSet<BasicBlock>> = HashMap::new();
        let dominators = body.basic_blocks.dominators();
        let predecessors = body.basic_blocks.predecessors();
        for (block, _) in body.basic_blocks.iter_enumerated() {
            dominance_frontier.entry(block).or_default();
        }

        // 遍历每个块
        for (block, block_data) in body.basic_blocks.iter_enumerated() {
            // 如果块有多个前驱，可能会出现在支配前沿
            if (predecessors[block].len() > 1) {
                let preds = body.basic_blocks.predecessors()[block].clone();

                for &pred in &preds {
                    let mut runner = pred;
                    while runner != dominators.immediate_dominator(block).unwrap() {
                        dominance_frontier.entry(runner).or_default().insert(block);
                        runner = dominators.immediate_dominator(runner).unwrap();
                    }
                }
            }
        }

        dominance_frontier
    }
    pub fn insert_phi_statment(&mut self) {
        // 初始化所有基本块的 phi 函数集合
        let mut phi_functions: HashMap<BasicBlock, HashSet<Local>> = HashMap::new();
        for bb in self.body.borrow().basic_blocks.indices() {
            phi_functions.insert(bb, HashSet::new());
        }
        let variables: Vec<Local> = self
            .local_assign_blocks
            .iter()
            .filter(|(_, blocks)| blocks.len() >= 2) // 只保留基本块数量大于等于 2 的条目
            .map(|(&local, _)| local) // 提取 Local
            .collect();
        print!("{:?}", variables);
        for var in &variables {
            // 获取变量的定义位置
            if let Some(def_blocks) = self.local_assign_blocks.get(var) {
                let mut worklist: VecDeque<BasicBlock> = def_blocks.iter().cloned().collect();
                let mut processed: HashSet<BasicBlock> = HashSet::new();

                while let Some(block) = worklist.pop_front() {
                    if let Some(df_blocks) = self.df.get(&block) {
                        for &df_block in df_blocks {
                            if !processed.contains(&df_block) {
                                phi_functions.get_mut(&df_block).unwrap().insert(*var);
                                processed.insert(df_block);
                                if self.local_assign_blocks[var].contains(&df_block) {
                                    worklist.push_back(df_block);
                                }
                            }
                        }
                    }
                }
            }
        }

        for (block, vars) in phi_functions {
            for var in vars {
                let decl = self.body.borrow().local_decls[var].clone();
                // let new_var = self.body.local_decls.push(decl);
                let predecessors = self.body.borrow().basic_blocks.predecessors()[block].clone();

                // 构造元组元素，使用占位变量
                let mut operands = IndexVec::with_capacity(predecessors.len());
                for _ in 0..predecessors.len() {
                    operands.push(Operand::Copy(Place::from(var)));
                } // 创建 phi 语句
                let phi_stmt = Statement {
                    source_info: SourceInfo::outermost(self.body.borrow().span),
                    kind: StatementKind::Assign(Box::new((
                        Place::from(var), // 左值是变量
                        Rvalue::Aggregate(
                            Box::new(AggregateKind::Tuple), // 元组类型
                            operands,
                        ),
                    ))),
                };

                // 插入到基本块的开头
                self.body.borrow_mut().basic_blocks_mut()[block]
                    .statements
                    .insert(0, phi_stmt);
            }
        }
    }
    fn extract_cfg_from_predecessors(body: &Body<'_>) -> HashMap<BasicBlock, Vec<BasicBlock>> {
        let mut cfg: HashMap<BasicBlock, Vec<BasicBlock>> = HashMap::new();

        // 遍历每个基本块
        for (block, _) in body.basic_blocks.iter_enumerated() {
            // 遍历每个块的前驱
            for &predecessor in body.basic_blocks.predecessors()[block].iter() {
                cfg.entry(predecessor).or_default().push(block);
            }
        }

        cfg
    }
    fn print_dominance_tree(
        dom_tree: &HashMap<BasicBlock, Vec<BasicBlock>>,
        current: BasicBlock,
        depth: usize,
    ) {
        // 打印当前块
        println!("\n{}{:?}", "  ".repeat(depth), current);

        // 遍历并递归打印子节点
        if let Some(children) = dom_tree.get(&current) {
            for &child in children {
                Self::print_dominance_tree(dom_tree, child, depth + 1);
            }
        }
    }
    pub fn is_phi_statement(statement: &Statement<'tcx>) -> bool {
        match &statement.kind {
            StatementKind::Assign(box (lhs, rhs)) => {
                // 1. 检查左值是 Local，且右值是 Aggregate 类型
                return matches!(rhs, Rvalue::Aggregate(_, _));
            }
            _ => {}
        }
        false
    }
    /// 主算法：执行 SSA 变量重命名
    pub fn rename_variables(&mut self) {
        // 初始化每个变量的 reachingDef
        for local in self.body.borrow().local_decls.indices() {
            self.reaching_def.insert(local, Some(local));
        }
        self.local_defination_block = Self::map_locals_to_definition_block(&self.body.borrow());
        print!("%%%%{:?}%%%%", self.reaching_def);
        print!(
            "\n local_defination_block after phi {:?}",
            self.local_defination_block
        );

        // 深度优先先序遍历支配树
        for bb in Self::depth_first_search_postorder(&self.dom_tree) {
            self.process_basic_block(bb);
        }
    }

    /// 处理单个基本块
    fn process_basic_block(&mut self, bb: BasicBlock) {
        // 获取基本块的可变引用
        let len = self.body.borrow().basic_blocks[bb].statements.len();
        for i in 0..len {
            self.rename_statement(i, bb);
        }

        // if let Some(terminator) = &mut block.terminator {
        //     self.rename_terminator(terminator);

        let successors: Vec<_> = self.body.borrow().basic_blocks[bb]
            .terminator()
            .successors()
            .collect();
        for succ_bb in successors {
            self.process_phi_functions(succ_bb);
        }
    }

    /// 处理后继块中的 φ 函数
    fn process_phi_functions(&mut self, bb: BasicBlock) {
        let mut binding = self.body.borrow_mut();
        let block = binding.basic_blocks_mut();
        let mut block = &mut block[bb];

        // 遍历 phi 变量
        // for statement in block.statements.iter_mut() {
        //     if let StatementKind::Assign(box (place, rvalue)) = &mut statement.kind {
        //         // 仅处理 Aggregate 类型
        //         if let Rvalue::Aggregate(_, operands) = rvalue {
        //             for operand in operands.iter_mut() {
        //                 if let Operand::Copy(src) | Operand::Move(src) = operand {
        //                     if let Some(local) = src.as_local() {
        //                         if let Some(def_stack) = self.reaching_def.get(&local) {
        //                             if let Some(current_def) = def_stack.last() {
        //                                 *src = Place::from(*current_def);
        //                             }
        //                         }
        //                     }
        //                 }
        //             }
        //         }
        //     }
        // }
        for statement in &mut block.statements {
            if let StatementKind::Assign(box (place, rvalue)) = &mut statement.kind {
                if let Rvalue::Aggregate(_, operands) = rvalue {
                    let mut unique_local: Option<Local> = None;
                    for operand in operands.iter_mut() {
                        if let Operand::Copy(src) | Operand::Move(src) = operand {
                            if let Some(local) = src.as_local() {
                                // 获取最新的 reaching definition
                                unique_local = Some(local);
                                if let Some(def_stack) = self.reaching_def.get(&local) {
                                    if let Some(&latest_def) = def_stack.into_iter().last() {
                                        *src = Place::from(latest_def); // 替换变量使用
                                    }
                                }
                            }
                        }
                    }

                    // 更新 reaching_def，使 `place` 绑定到新的变量版本
                    if let Some(new_local) = place.as_local() {
                        self.reaching_def
                            .insert(unique_local.unwrap(), Some(new_local));
                    }
                }
            }
        }
    }

    /// 创建一个新的变量版本

    // fn create_fresh_variable(&self, local: Local, body: &mut Body<'tcx>) -> Local {
    //     let new_local = body.local_decls.push(body.local_decls[local].clone());
    //     new_local
    // }
    pub fn rename_statement(&mut self, i: usize, bb: BasicBlock) {
        for statement in self.body.clone().borrow_mut().basic_blocks_mut()[bb]
            .statements
            .iter_mut()
        {
            // let rc_stat = Rc::new(RefCell::new(statement));
            let is_phi = Self::is_phi_statement(statement);
            match &mut statement.kind {
                // 1. 赋值语句: 变量使用（右值），变量定义（左值）
                StatementKind::Assign(box (place, rvalue)) => {
                    {
                        if !is_phi {
                            // self.update_reachinf_def(&place.local, &bb);
                            self.replace_rvalue(rvalue);
                        } else {
                            //每个定义生成的变量
                            // self.replace_place(place,rc_stat.clone());
                        }
                    }
                }
                // 2. FakeRead: 变量使用
                // StatementKind::FakeRead(_, place)
                StatementKind::Deinit(place) | StatementKind::SetDiscriminant { place, .. } => {
                    // let place_mut = unsafe { &mut *(place as *const _ as *mut _) };

                    // self.replace_place(place.as_mut());
                }
                // 3. StorageLive: 变量定义
                StatementKind::StorageLive(local) => {
                    // self.rename_local_def(*local);
                }
                // 4. StorageDead: 变量使用
                StatementKind::StorageDead(local) => {
                    // self.replace_local(local);
                }
                _ => {}
            }
        }
    }

    fn rename_terminator(&mut self, terminator: &mut Terminator<'tcx>) {
        // match &mut terminator.kind {
        //     // 1. 函数调用: 参数使用，返回值定义
        //     TerminatorKind::Call { args, destination, .. } => {
        //         for operand in args {
        //             self.replace_operand(operand);
        //         }
        //         if let Some((place, _)) = destination {
        //             self.rename_def(place);
        //         }
        //     }
        //     // 2. 断言: 变量使用
        //     TerminatorKind::Assert { cond, .. } => {
        //         self.replace_operand(cond);
        //     }
        //     // 3. Drop: 变量使用
        //     TerminatorKind::Drop { place, .. } => {
        //         self.replace_place(place);
        //     }
        //     // 4. SwitchInt: 变量使用
        //     TerminatorKind::SwitchInt { discr, .. } => {
        //         self.replace_operand(discr);
        //     }
        //     _ => {}
        // }
    }

    fn replace_rvalue(&mut self, rvalue: &mut Rvalue<'tcx>) {
        match rvalue {
            Rvalue::Use(operand)
            | Rvalue::Repeat(operand, _)
            | Rvalue::UnaryOp(_, operand)
            | Rvalue::Cast(_, operand, _)
            | Rvalue::ShallowInitBox(operand, _) => {
                self.replace_operand(operand);
            }
            Rvalue::BinaryOp(_, box (lhs, rhs)) | Rvalue::BinaryOp(_, box (lhs, rhs)) => {
                self.replace_operand(lhs);
                self.replace_operand(rhs);
            }
            Rvalue::Aggregate(_, operands) => {
                for operand in operands {
                    self.replace_operand(operand);
                }
            }
            _ => {}
        }
    }

    fn replace_operand(&mut self, operand: &mut Operand<'tcx>) {
        if let Operand::Copy(mut place) | Operand::Move(mut place) = operand {
            self.replace_place(&mut place);
        }
    }

    fn replace_place(&mut self, place: &mut Place<'tcx>) {
        if let Some(reaching_local) = self.reaching_def.get(&place.local) {
            let local = reaching_local.unwrap().clone();
            place.local = local;
            //  *place = Place::from(local);
        }
    }

    fn rename_def(&mut self, place: &mut Place<'tcx>) {
        if let Some(local) = place.as_local() {
            // let new_local = self.create_fresh_variable(local);
            // self.reaching_def.entry(local).or_default().push(new_local);
            // *place = Place::from(new_local);
        }
    }

    fn rename_local_def(&mut self, place: &mut Place<'tcx>) {
        let old_local = place.as_local().unwrap();
        let new_local = self.create_fresh_variable(old_local);
        print!("fuck!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
        *place = Place::from(new_local);

        // self.reaching_def
        //     .entry(old_local)
        //     .or_default()
        //     .replace(Some(old_local));
    }

    fn replace_local(&self, local: &mut Local) {
        if let Some(reaching_local) = self.reaching_def.get(local) {
            // if let Some(latest) = stack {
            //     *local = latest;
            // }
        }
    }

    fn create_fresh_variable(&mut self, local: Local) -> Local {
        let mut binding = self.body.borrow_mut();
        let new_local_decl = binding.local_decls[local].clone();
        let new_local = binding.local_decls.push(new_local_decl);
        new_local
    }
    pub fn dominates_(&self, def_bb: &BasicBlock, bb: &BasicBlock) -> bool {
        // 使用一个集合来追踪所有被 def_bb 支配的基本块
        let mut visited = HashSet::new();

        // 从 def_bb 出发，遍历其子树
        let mut stack = self.dom_tree.get(def_bb).unwrap().clone();
        while let Some(block) = stack.pop() {
            if !visited.insert(block) {
                continue;
            }

            // 如果当前块是 bb，说明 def_bb 支配了 bb
            if block == *bb {
                return true;
            }

            // 将所有子节点加入栈中，继续遍历
            if let Some(children) = self.dom_tree.get(&block) {
                stack.extend(children);
            }
        }

        false
    }
    fn update_reachinf_def(&mut self, local: &Local, bb: &BasicBlock) {
        let def_bb = self.local_defination_block[local];
        let mut r = self.reaching_def[local];
        while !(self.dominates_(&def_bb, bb) || r == None) {
            r = self.reaching_def[&r.unwrap()];
        }
        if let Some(entry) = self.reaching_def.get_mut(local) {
            *entry = r.clone();
        }
    }
}
