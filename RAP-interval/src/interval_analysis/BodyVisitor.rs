use super::ConstraintGraph::ConstraintGraph;

pub struct BodyVisitor<'tcx> {
    pub tcx: TyCtxt<'tcx>,
    pub def_id: DefId,
    pub CGT: ConstraintGraph,
    pub body: &'tcx Body,
}
impl<'tcx> BodyVisitor<'tcx> {
    pub fn new(tcx: TyCtxt<'tcx>, def_id: DefId) -> Self {
        let body = tcx.optimized_mir(def_id);
        Self {
            tcx,
            def_id,
            CGT: ConstraintGraph::new(),
            body: body,
        }
    }
    pub fn analysis(&self) {
        self.CGT = self.CGT.build_graph(self.body);
        CGT.build_VarNodes();
        CGT.findIntervals();
    }
}
