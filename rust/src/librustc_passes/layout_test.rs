use rustc::hir;
use rustc::hir::def_id::DefId;
use rustc::hir::itemlikevisit::ItemLikeVisitor;
use rustc::hir::ItemKind;
use rustc::ty::layout::HasDataLayout;
use rustc::ty::layout::HasTyCtxt;
use rustc::ty::layout::LayoutOf;
use rustc::ty::layout::TargetDataLayout;
use rustc::ty::layout::TyLayout;
use rustc::ty::ParamEnv;
use rustc::ty::Ty;
use rustc::ty::TyCtxt;
use syntax::ast::Attribute;

pub fn test_layout<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>) {
    if tcx.features().rustc_attrs {
        // if the `rustc_attrs` feature is not enabled, don't bother testing layout
        tcx.hir()
            .krate()
            .visit_all_item_likes(&mut VarianceTest { tcx });
    }
}

struct VarianceTest<'a, 'tcx: 'a> {
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
}

impl<'a, 'tcx> ItemLikeVisitor<'tcx> for VarianceTest<'a, 'tcx> {
    fn visit_item(&mut self, item: &'tcx hir::Item) {
        let item_def_id = self.tcx.hir().local_def_id_from_hir_id(item.hir_id);

        if let ItemKind::Ty(..) = item.node {
            for attr in self.tcx.get_attrs(item_def_id).iter() {
                if attr.check_name("rustc_layout") {
                    self.dump_layout_of(item_def_id, item, attr);
                }
            }
        }
    }

    fn visit_trait_item(&mut self, _: &'tcx hir::TraitItem) {}
    fn visit_impl_item(&mut self, _: &'tcx hir::ImplItem) {}
}

impl<'a, 'tcx> VarianceTest<'a, 'tcx> {
    fn dump_layout_of(&self, item_def_id: DefId, item: &hir::Item, attr: &Attribute) {
        let tcx = self.tcx;
        let param_env = self.tcx.param_env(item_def_id);
        let ty = self.tcx.type_of(item_def_id);
        match self.tcx.layout_of(param_env.and(ty)) {
            Ok(ty_layout) => {
                // Check out the `#[rustc_layout(..)]` attribute to tell what to dump.
                // The `..` are the names of fields to dump.
                let meta_items = attr.meta_item_list().unwrap_or_default();
                for meta_item in meta_items {
                    match meta_item.name_or_empty().get() {
                        "abi" => {
                            self.tcx
                                .sess
                                .span_err(item.span, &format!("abi: {:?}", ty_layout.abi));
                        }

                        "align" => {
                            self.tcx
                                .sess
                                .span_err(item.span, &format!("align: {:?}", ty_layout.align));
                        }

                        "size" => {
                            self.tcx
                                .sess
                                .span_err(item.span, &format!("size: {:?}", ty_layout.size));
                        }

                        "homogeneous_aggregate" => {
                            self.tcx.sess.span_err(
                                item.span,
                                &format!(
                                    "homogeneous_aggregate: {:?}",
                                    ty_layout
                                        .homogeneous_aggregate(&UnwrapLayoutCx { tcx, param_env }),
                                ),
                            );
                        }

                        name => {
                            self.tcx.sess.span_err(
                                meta_item.span(),
                                &format!("unrecognized field name `{}`", name),
                            );
                        }
                    }
                }
            }

            Err(layout_error) => {
                self.tcx
                    .sess
                    .span_err(item.span, &format!("layout error: {:?}", layout_error));
            }
        }
    }
}

struct UnwrapLayoutCx<'me, 'tcx> {
    tcx: TyCtxt<'me, 'tcx, 'tcx>,
    param_env: ParamEnv<'tcx>,
}

impl<'me, 'tcx> LayoutOf for UnwrapLayoutCx<'me, 'tcx> {
    type Ty = Ty<'tcx>;
    type TyLayout = TyLayout<'tcx>;

    fn layout_of(&self, ty: Ty<'tcx>) -> Self::TyLayout {
        self.tcx.layout_of(self.param_env.and(ty)).unwrap()
    }
}

impl<'me, 'tcx> HasTyCtxt<'tcx> for UnwrapLayoutCx<'me, 'tcx> {
    fn tcx<'a>(&'a self) -> TyCtxt<'a, 'tcx, 'tcx> {
        self.tcx
    }
}

impl<'me, 'tcx> HasDataLayout for UnwrapLayoutCx<'me, 'tcx> {
    fn data_layout(&self) -> &TargetDataLayout {
        self.tcx.data_layout()
    }
}
