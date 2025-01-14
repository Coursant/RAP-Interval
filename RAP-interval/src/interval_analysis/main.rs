#![feature(rustc_private)]
// tidy-alphabetical-start
#![feature(assert_matches)]
#![feature(box_patterns)]
#![feature(const_type_name)]
#![feature(cow_is_borrowed)]
#![feature(decl_macro)]
#![feature(if_let_guard)]
#![feature(impl_trait_in_assoc_type)]
#![feature(is_sorted)]
#![feature(let_chains)]
#![feature(map_try_insert)]
#![feature(never_type)]
#![feature(option_get_or_insert_default)]
#![feature(round_char_boundary)]
#![feature(try_blocks)]
#![feature(yeet_expr)]
// tidy-alphabetical-end

#[macro_use]
mod ssa;

extern crate rustc_codegen_ssa;
extern crate rustc_const_eval;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_index;
extern crate rustc_interface;
extern crate rustc_metadata;
extern crate rustc_middle;
extern crate rustc_mir_dataflow;
extern crate rustc_mir_transform;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_target;
extern crate tracing;

// use rustc_mir_transform::ssa::SsaLocals;
use crate::ssa::SsaLocals;
use rustc_data_structures::graph::{dominators, Predecessors};
use rustc_driver::Compilation;
use rustc_driver::{Callbacks, RunCompiler};
use rustc_hir::def_id::LocalDefId;
use rustc_index::IndexVec;
use rustc_interface::{interface::Compiler, Queries};
use rustc_middle::mir::pretty::*;
use rustc_middle::mir::*;
use rustc_middle::{
    mir::{visit::Visitor, Body, Local, Location},
    ty::TyCtxt,
};
use rustc_target::abi::FieldIdx;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

use rustc_data_structures::fx::FxHashMap;

struct SSAContext {
    reaching_def: FxHashMap<Local, Vec<Local>>, // 每个变量的定义栈
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

impl<'tcx> SSAContext {
    fn replace_with_latest_def(&mut self, operand: &mut Operand<'tcx>) {
        if let Operand::Copy(place) | Operand::Move(place) = operand {
            if let Some(local) = place.as_local() {
                if let Some(def_stack) = self.reaching_def.get(&local) {
                    if let Some(current_def) = def_stack.last() {
                        *place = Place::from(*current_def);
                    }
                }
            }
        }
    }
    pub fn new() -> Self {
        SSAContext {
            reaching_def: FxHashMap::default(),
        }
    }

    /// 主算法：执行 SSA 变量重命名
    pub fn rename_variables(
        &mut self,
        tcx: TyCtxt<'tcx>,
        body: &mut Body<'tcx>,
        dominator_tree: &HashMap<BasicBlock, Vec<BasicBlock>>,
    ) {
        // 初始化每个变量的 reachingDef
        for local in body.local_decls.indices() {
            self.reaching_def.insert(local, vec![local]);
        }

        // 深度优先先序遍历支配树
        for bb in depth_first_search_postorder(dominator_tree) {
            self.process_basic_block(bb, body);
        }
        // for succ_bb in body.basic_blocks[bb].terminator().successors() {
        //     self.process_phi_functions(succ_bb, body);
        // }
    }

    /// 处理单个基本块
    fn process_basic_block(&mut self, bb: BasicBlock, body: &mut Body<'tcx>) {
        let statements = &mut body.basic_blocks_mut()[bb].statements;

        // 线性处理基本块中的每条指令
        for stmt in statements.iter_mut() {
            match &mut stmt.kind {
                StatementKind::Assign(box (place, rvalue)) => {
                    // 仅处理非聚合类型的赋值
                    if !matches!(rvalue, Rvalue::Aggregate(..)) {
                        match rvalue {
                            Rvalue::Use(ref mut operand) => {
                                self.replace_with_latest_def(operand);
                            }
                            Rvalue::BinaryOp(op, box (ref mut operand1, ref mut operand2)) => {
                                self.replace_with_latest_def(operand1);
                                self.replace_with_latest_def(operand2);
                            }
                            Rvalue::UnaryOp(op, ref mut operand) => {
                                self.replace_with_latest_def(operand);
                            }
                            Rvalue::Repeat(operand, _) => todo!(),
                            Rvalue::Ref(region, borrow_kind, place) => todo!(),
                            Rvalue::ThreadLocalRef(def_id) => todo!(),
                            Rvalue::RawPtr(mutability, place) => todo!(),
                            Rvalue::Len(place) => todo!(),
                            Rvalue::Cast(cast_kind, operand, ty) => todo!(),
                            Rvalue::NullaryOp(null_op, ty) => todo!(),
                            Rvalue::Discriminant(place) => todo!(),
                            Rvalue::Aggregate(aggregate_kind, index_vec) => todo!(),
                            Rvalue::ShallowInitBox(operand, ty) => todo!(),
                            Rvalue::CopyForDeref(place) => todo!(),
                        }
                        // 遍历 rvalue 中的操作数，并执行变量重命名
                        // for operand in rvalue.operands_mut() {
                        //     if let Operand::Copy(place) | Operand::Move(place) = operand {
                        //         if let Some(local) = place.as_local() {
                        //             // 替换使用变量为其最新定义
                        //             self.update_reaching_def(local);
                        //             if let Some(current_def) =
                        //                 self.reaching_def.get(&local).and_then(|stack| stack.last())
                        //             {
                        //                 *place = Place::from(*current_def);
                        //             }
                        //         }
                        //     }
                        // }

                    }
                    if place.as_local().is_some() {
                      // replace_with_latest_def(place);
                        // self.rename_def(place, body);
                    }
                }
                _ => {}
            }
        }

        // 处理后继块中的 φ 函数
        let successors: Vec<_> = body.basic_blocks_mut()[bb].terminator().successors().collect();
        for successor in successors {
            self.process_phi_functions(successor, body);
        }
    }

    /// 重命名指令中的使用变量
    // fn rename_uses(&mut self, rvalue: &mut Rvalue<'tcx>) {
    //     // 遍历 Rvalue 中的变量使用
    //     for operand in rvalue.operands_mut() {
    //         if let Operand::Copy(place) | Operand::Move(place) = operand {
    //             if let Some(local) = place.as_local() {
    //                 if let Some(def_stack) = self.reaching_def.get(&local) {
    //                     if let Some(current_def) = def_stack.last() {
    //                         *place = Place::from(*current_def);
    //                     }
    //                 }
    //             }
    //         }
    //     }
    // }

    // /// 为指令中定义的变量分配新版本
    fn rename_def(&mut self, place: &mut Place<'tcx>, body: &mut Body<'tcx>) {
        if let Some(local) = place.as_local() {
            let new_local = self.create_fresh_variable(local, body);
            if let Some(def_stack) = self.reaching_def.get_mut(&local) {
                def_stack.push(new_local);
            }
            *place = Place::from(new_local);
        }
    }

    /// 处理后继块中的 φ 函数
    fn process_phi_functions(&mut self, bb: BasicBlock, body: &mut Body<'tcx>) {
        // if let Some(Terminator {
        //     kind: TerminatorKind::Call { args, .. },
        //     ..
        // }) = &body.basic_blocks[bb].terminator
        // {
        //     for arg in args {
        //         if let Operand::Copy(place) | Operand::Move(place) = arg {
        //             if let Some(local) = place.as_local() {
        //                 if let Some(def_stack) = self.reaching_def.get(&local) {
        //                     if let Some(current_def) = def_stack.last() {
        //                         *place = Place::from(*current_def);
        //                     }
        //                 }
        //             }
        //         }
        //     }
        // }
    }

    /// 创建一个新的变量版本
fn create_fresh_variable(&mut self, local: Local, body: &mut Body<'_>) -> Local {
    // 假设新的 Local 分配基于现有的数量
    let new_local_index = body.local_decls.len();
    
    // 创建一个新的变量声明
    let new_decl = body.local_decls[local].clone();
    let new_local = body.local_decls.push(new_decl);
    new_local
}

}

pub struct SSAtransform<'tcx> {
    /// 保存每个变量的当前版本
    version_map: IndexVec<Local, usize>,
    /// 保存每个基本块的前驱块信息
    phi_inserts: IndexVec<BasicBlock, Vec<(Local, Vec<Operand<'tcx>>)>>,
}

impl<'tcx> SSAtransform<'tcx> {
    /// 创建一个新的 `SSAtransform` 实例
    pub fn new(body: &Body<'tcx>) -> Self {
        Self {
            version_map: IndexVec::from_elem(0, &body.local_decls),
            phi_inserts: IndexVec::from_elem(Vec::new(), &body.basic_blocks),
        }
    }

    /// 执行 SSA 转换
    pub fn apply(&mut self, tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
        self.collect_versions_and_phi_places(body);
        self.insert_phi_functions(body);
    }

    /// 收集变量版本和需要插入 Phi 函数的位置
    fn collect_versions_and_phi_places(&mut self, body: &Body<'tcx>) {
        for (bb_idx, bb) in body.basic_blocks.iter_enumerated() {
            for stmt in &bb.statements {
                if let StatementKind::Assign(box (place, _)) = &stmt.kind {
                    if let Some(local) = place.as_local() {
                        // 更新变量版本
                        self.version_map[local] += 1;
                    }
                }
            }

            // if let Some(terminator) = &bb.terminator {
            //     for &target in terminator.successors() {
            //         for local in self.version_map.indices() {
            //             // 收集需要在目标块插入的变量值
            //             let current_operand = Operand::Copy(
            //                 // Place::from(local).with_field(None, self.version_map[local]),
            //             );
            //             self.phi_inserts[target].push((local, vec![current_operand]));
            //         }
            //     }
            // }
        }
    }

    /// 插入 Phi 函数到每个目标块的头部
    fn insert_phi_functions(&mut self, body: &mut Body<'tcx>) {
        for (bb, phi_list) in self.phi_inserts.iter_enumerated() {
            // for (local, incoming_operands) in phi_list {
            //     let phi_statement = Statement {
            //         source_info: SourceInfo::outermost(body.span),
            //         kind: StatementKind::Assign(Box::new((
            //             Place::from(*local),
            //             Rvalue::Aggregate(Box::new(AggregateKind::Tuple), incoming_operands.clone()),
            //         ))),
            //     };

            //     // 在块的头部插入 Phi 函数
            //     body.basic_blocks[bb].statements.insert(0, phi_statement);
            // }
        }
    }
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
fn map_locals_to_assign_blocks<'tcx>(mir: &Body<'tcx>) -> HashMap<Local, HashSet<BasicBlock>> {
    let mut local_to_blocks: HashMap<Local, HashSet<BasicBlock>> = HashMap::new();

    for (bb, data) in mir.basic_blocks.iter_enumerated() {
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
fn print_dominance_tree(
    dom_tree: &HashMap<BasicBlock, Vec<BasicBlock>>,
    current: BasicBlock,
    depth: usize,
) {
    // 打印当前块
    println!("{}{:?}", "  ".repeat(depth), current);

    // 遍历并递归打印子节点
    if let Some(children) = dom_tree.get(&current) {
        for &child in children {
            print_dominance_tree(dom_tree, child, depth + 1);
        }
    }
}
fn insert_phi_statment(
    body: &mut Body<'_>,                                           
    dominance_frontier: &HashMap<BasicBlock, HashSet<BasicBlock>>, 
    local_assign_blocks: HashMap<Local, HashSet<BasicBlock>>,      
) {
    // 初始化所有基本块的 phi 函数集合
    let mut phi_functions: HashMap<BasicBlock, HashSet<Local>> = HashMap::new();
    for bb in body.basic_blocks.indices() {
        phi_functions.insert(bb, HashSet::new());
    }
    let variables: Vec<Local> = local_assign_blocks
        .iter()
        .filter(|(_, blocks)| blocks.len() >= 2) // 只保留基本块数量大于等于 2 的条目
        .map(|(&local, _)| local) // 提取 Local
        .collect();
    print!("{:?}", variables);
    for var in &variables {
        // 获取变量的定义位置
        if let Some(def_blocks) = local_assign_blocks.get(var) {
            let mut worklist: VecDeque<BasicBlock> = def_blocks.iter().cloned().collect();
            let mut processed: HashSet<BasicBlock> = HashSet::new();

            while let Some(block) = worklist.pop_front() {
                if let Some(df_blocks) = dominance_frontier.get(&block) {
                    for &df_block in df_blocks {
                        if !processed.contains(&df_block) {
                            phi_functions.get_mut(&df_block).unwrap().insert(*var);
                            processed.insert(df_block);
                            if local_assign_blocks[var].contains(&df_block) {
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
            let decl = body.local_decls[var].clone();
            let new_var = body.local_decls.push(decl);
            let predecessors = body.basic_blocks.predecessors()[block].clone();

            // 构造元组元素，使用占位变量
            let mut operands = IndexVec::with_capacity(predecessors.len());
            for _ in 0..predecessors.len() {
                operands.push(Operand::Copy(Place::from(var)));
            } // 创建 phi 语句
            let phi_stmt = Statement {
                source_info: SourceInfo::outermost(body.span),
                kind: StatementKind::Assign(Box::new((
                    Place::from(new_var), // 左值是变量
                    Rvalue::Aggregate(
                        Box::new(AggregateKind::Tuple), // 元组类型
                        operands,                       
                    ),
                ))),
            };

            // 插入到基本块的开头
            body.basic_blocks_mut()[block]
                .statements
                .insert(0, phi_stmt);
        }
    }
}

// 该函数演示如何在编译过程中，对给定函数体的MIR进行数据流级别遍历。
fn analyze_mir(tcx: TyCtxt<'_>, def_id: LocalDefId) {
    let mut body = tcx.optimized_mir(def_id).clone();
    // let mut visitor = LocalUseVisitor { tcx, &body };
    // visitor.visit_body(&body);
    let param_env = tcx.param_env_reveal_all_normalized(body.source.def_id());
    let ssa = SsaLocals::new(tcx, &body, param_env);
    let dominators = body.basic_blocks.dominators();
    let cfg = extract_cfg_from_predecessors(&body);
    let variables = body.local_decls.indices().collect::<Vec<_>>();
    println!("{:?}", cfg);
    println!("{:?}", dominators);
    println!("!!!!!!!!!!!!!!!!!!!!!!!!");
    let dom_tree = construct_dominance_tree(&body);
    print_dominance_tree(&dom_tree, START_BLOCK, 0);
    let df = compute_dominance_frontier(&body, &dom_tree);
    print!("{:?}", df);
    println!("!!!!!!!!!!!!!!!!!!!!!!!!");
    let local_assign_blocks = map_locals_to_assign_blocks(&body);

    print!("{:?}", local_assign_blocks);
    // let mut visitor = LocalUseVisitor { tcx, body };
    insert_phi_statment(&mut body, &df, local_assign_blocks);

    // visitor.visit_body(&body);
    let mut file = File::create(format!("mir_{:?}.txt", def_id)).unwrap();
    let mut w = io::BufWriter::new(&mut file);
    write_mir_pretty(tcx, None, &mut w).unwrap();
    let mut file2 = File::create(format!("phi_mir_{:?}.txt", def_id)).unwrap();
    let mut w2 = io::BufWriter::new(&mut file2);
    write_mir_fn(
        tcx,
        &body,
        &mut |_, _| Ok(()),
        &mut w2,
        PrettyPrintMirOptions::from_cli(tcx),
    )
    .unwrap();
}

struct MyDataflowCallbacks;

impl Callbacks for MyDataflowCallbacks {
    fn after_analysis<'tcx>(
        &mut self,
        compiler: &Compiler,
        queries: &'tcx Queries<'tcx>,
    ) -> Compilation {
        let mut tcx = queries.global_ctxt().unwrap();
        tcx.enter(|tcx| {
            // 获取 main 函数对应的LocalDefId，仅做示例
            if let Some(def_id) = tcx
                .hir()
                .body_owners()
                .find(|id| tcx.def_path_str(*id) == "main")
            {
                analyze_mir(tcx, def_id);
            }
        });
        Compilation::Continue
    }
}

// 在main函数中使用rustc_driver手动调用编译过程，并运行回调进行数据流分析
fn main() {
    let args = vec![
        String::from("rustc"),
        String::from("src/test.rs"),
        String::from("--crate-type=bin"),
        String::from("-Zalways-encode-mir"),
    ];

    RunCompiler::new(&args, &mut MyDataflowCallbacks)
        .run()
        .unwrap();
}

// ...existing code...