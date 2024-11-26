pub mod domain;
pub mod ConstraintGraph;
use rustc_hir::def::DefKind;
use rustc_middle::bug;
use rustc_middle::mir::interpret::{InterpResult, Scalar};
use rustc_middle::mir::visit::{MutVisitor, PlaceContext, Visitor};
use rustc_middle::mir::*;
use rustc_middle::ty::layout::{HasParamEnv, LayoutOf};
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_middle::mir::LocalDecls;
pub struct IntervalAnalysis{


        map: Map,
        tcx: TyCtxt<'tcx>,
        local_decls: &'a LocalDecls<'tcx>,
        // ecx: InterpCx<'tcx, DummyMachine>,
        // param_env: ty::ParamEnv<'tcx>,
    
    
}
impl<'a, 'tcx> IntervalAnalysis<'a, 'tcx> {
    pub fn new(tcx: TyCtxt<'tcx>, body: &'a Body<'tcx>, map: Map) -> Self {
        let param_env = tcx.param_env_reveal_all_normalized(body.source.def_id());
        Self {
            map,
            tcx,
            local_decls: &body.local_decls,
            ecx: InterpCx::new(tcx, DUMMY_SP, param_env, DummyMachine),
            param_env: param_env,
        }
    }
}