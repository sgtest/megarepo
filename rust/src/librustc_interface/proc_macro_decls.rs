use rustc::hir::itemlikevisit::ItemLikeVisitor;
use rustc::hir::def_id::{CrateNum, DefId, LOCAL_CRATE};
use rustc::hir;
use rustc::ty::TyCtxt;
use rustc::ty::query::Providers;
use syntax::attr;

pub fn find<'tcx>(tcx: TyCtxt<'_, 'tcx, 'tcx>) -> Option<DefId> {
    tcx.proc_macro_decls_static(LOCAL_CRATE)
}

fn proc_macro_decls_static<'tcx>(
    tcx: TyCtxt<'_, 'tcx, 'tcx>,
    cnum: CrateNum,
) -> Option<DefId> {
    assert_eq!(cnum, LOCAL_CRATE);

    let mut finder = Finder { decls: None };
    tcx.hir().krate().visit_all_item_likes(&mut finder);

    finder.decls.map(|id| tcx.hir().local_def_id_from_hir_id(id))
}

struct Finder {
    decls: Option<hir::HirId>,
}

impl<'v> ItemLikeVisitor<'v> for Finder {
    fn visit_item(&mut self, item: &hir::Item) {
        if attr::contains_name(&item.attrs, "rustc_proc_macro_decls") {
            self.decls = Some(item.hir_id);
        }
    }

    fn visit_trait_item(&mut self, _trait_item: &hir::TraitItem) {
    }

    fn visit_impl_item(&mut self, _impl_item: &hir::ImplItem) {
    }
}

pub(crate) fn provide(providers: &mut Providers<'_>) {
    *providers = Providers {
        proc_macro_decls_static,
        ..*providers
    };
}
