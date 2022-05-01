use crate::MirPass;
use rustc_index::vec::IndexVec;
use rustc_middle::mir::patch::MirPatch;
use rustc_middle::mir::visit::{MutVisitor, PlaceContext};
use rustc_middle::mir::*;
use rustc_middle::ty::TyCtxt;
pub struct Derefer;

pub struct DerefChecker<'tcx> {
    tcx: TyCtxt<'tcx>,
    patcher: MirPatch<'tcx>,
    local_decls: IndexVec<Local, LocalDecl<'tcx>>,
}

impl<'tcx> MutVisitor<'tcx> for DerefChecker<'tcx> {
    fn tcx(&self) -> TyCtxt<'tcx> {
        self.tcx
    }

    fn visit_place(&mut self, place: &mut Place<'tcx>, _: PlaceContext, loc: Location) {
        let mut place_local = place.local;
        let mut last_len = 0;
        let mut last_deref_idx = 0;

        let mut prev_temp: Option<Local> = None;

        for (idx, (p_ref, p_elem)) in place.iter_projections().enumerate() {
            if p_elem == ProjectionElem::Deref && !p_ref.projection.is_empty() {
                last_deref_idx = idx;
            }
        }

        for (idx, (p_ref, p_elem)) in place.iter_projections().enumerate() {
            if p_elem == ProjectionElem::Deref && !p_ref.projection.is_empty() {
                let ty = p_ref.ty(&self.local_decls, self.tcx).ty;
                let temp = self.patcher.new_local_with_info(
                    ty,
                    self.local_decls[p_ref.local].source_info.span,
                    Some(Box::new(LocalInfo::DerefTemp)),
                );

                self.patcher.add_statement(loc, StatementKind::StorageLive(temp));

                // We are adding current p_ref's projections to our
                // temp value, excluding projections we already covered.
                let deref_place = Place::from(place_local)
                    .project_deeper(&p_ref.projection[last_len..], self.tcx);

                self.patcher.add_assign(
                    loc,
                    Place::from(temp),
                    Rvalue::Use(Operand::Move(deref_place)),
                );
                place_local = temp;
                last_len = p_ref.projection.len();

                // Change `Place` only if we are actually at the Place's last deref
                if idx == last_deref_idx {
                    let temp_place =
                        Place::from(temp).project_deeper(&place.projection[idx..], self.tcx);
                    *place = temp_place;
                }

                // We are destroying the previous temp since it's no longer used.
                if let Some(prev_temp) = prev_temp {
                    self.patcher.add_statement(loc, StatementKind::StorageDead(prev_temp));
                }

                prev_temp = Some(temp);
            }
        }

        // Since we won't be able to reach final temp, we destroy it outside the loop.
        if let Some(prev_temp) = prev_temp {
            let last_loc = Location { block: loc.block, statement_index: loc.statement_index + 1 };
            self.patcher.add_statement(last_loc, StatementKind::StorageDead(prev_temp));
        }
    }
}

pub fn deref_finder<'tcx>(tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
    let patch = MirPatch::new(body);
    let mut checker = DerefChecker { tcx, patcher: patch, local_decls: body.local_decls.clone() };

    for (bb, data) in body.basic_blocks_mut().iter_enumerated_mut() {
        checker.visit_basic_block_data(bb, data);
    }

    checker.patcher.apply(body);
}

impl<'tcx> MirPass<'tcx> for Derefer {
    fn run_pass(&self, tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
        deref_finder(tcx, body);
    }
}
