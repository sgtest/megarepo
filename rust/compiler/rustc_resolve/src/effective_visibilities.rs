use crate::{ImportKind, NameBinding, NameBindingKind, Resolver, ResolverTree};
use rustc_ast::ast;
use rustc_ast::visit;
use rustc_ast::visit::Visitor;
use rustc_ast::Crate;
use rustc_ast::EnumDef;
use rustc_data_structures::intern::Interned;
use rustc_hir::def_id::LocalDefId;
use rustc_hir::def_id::CRATE_DEF_ID;
use rustc_middle::middle::privacy::{EffectiveVisibilities, EffectiveVisibility, Level};
use rustc_middle::ty::Visibility;

type ImportId<'a> = Interned<'a, NameBinding<'a>>;

#[derive(Clone, Copy)]
enum ParentId<'a> {
    Def(LocalDefId),
    Import(ImportId<'a>),
}

impl ParentId<'_> {
    fn level(self) -> Level {
        match self {
            ParentId::Def(_) => Level::Direct,
            ParentId::Import(_) => Level::Reexported,
        }
    }
}

pub struct EffectiveVisibilitiesVisitor<'r, 'a> {
    r: &'r mut Resolver<'a>,
    /// While walking import chains we need to track effective visibilities per-binding, and def id
    /// keys in `Resolver::effective_visibilities` are not enough for that, because multiple
    /// bindings can correspond to a single def id in imports. So we keep a separate table.
    import_effective_visibilities: EffectiveVisibilities<ImportId<'a>>,
    changed: bool,
}

impl<'r, 'a> EffectiveVisibilitiesVisitor<'r, 'a> {
    /// Fills the `Resolver::effective_visibilities` table with public & exported items
    /// For now, this doesn't resolve macros (FIXME) and cannot resolve Impl, as we
    /// need access to a TyCtxt for that.
    pub fn compute_effective_visibilities<'c>(r: &'r mut Resolver<'a>, krate: &'c Crate) {
        let mut visitor = EffectiveVisibilitiesVisitor {
            r,
            import_effective_visibilities: Default::default(),
            changed: false,
        };

        visitor.update(CRATE_DEF_ID, CRATE_DEF_ID);
        visitor.set_bindings_effective_visibilities(CRATE_DEF_ID);

        while visitor.changed {
            visitor.changed = false;
            visit::walk_crate(&mut visitor, krate);
        }

        // Update visibilities for import def ids. These are not used during the
        // `EffectiveVisibilitiesVisitor` pass, because we have more detailed binding-based
        // information, but are used by later passes. Effective visibility of an import def id
        // is the maximum value among visibilities of bindings corresponding to that def id.
        for (binding, eff_vis) in visitor.import_effective_visibilities.iter() {
            let NameBindingKind::Import { import, .. } = binding.kind else { unreachable!() };
            if let Some(node_id) = import.id() {
                let mut update = |node_id| {
                    r.effective_visibilities.update_eff_vis(
                        r.local_def_id(node_id),
                        eff_vis,
                        ResolverTree(&r.definitions, &r.crate_loader),
                    )
                };
                update(node_id);
                if let ImportKind::Single { additional_ids: (id1, id2), .. } = import.kind {
                    // In theory all the single import IDs have individual visibilities and
                    // effective visibilities, but in practice these IDs go straight to HIR
                    // where all their few uses assume that their (effective) visibility
                    // applies to the whole syntactic `use` item. So they all get the same
                    // value which is the maximum of all bindings. Maybe HIR for imports
                    // shouldn't use three IDs at all.
                    if id1 != ast::DUMMY_NODE_ID {
                        update(id1);
                    }
                    if id2 != ast::DUMMY_NODE_ID {
                        update(id2);
                    }
                }
            }
        }

        info!("resolve::effective_visibilities: {:#?}", r.effective_visibilities);
    }

    fn nearest_normal_mod(&mut self, def_id: LocalDefId) -> LocalDefId {
        self.r.get_nearest_non_block_module(def_id.to_def_id()).nearest_parent_mod().expect_local()
    }

    /// Update effective visibilities of bindings in the given module,
    /// including their whole reexport chains.
    fn set_bindings_effective_visibilities(&mut self, module_id: LocalDefId) {
        assert!(self.r.module_map.contains_key(&&module_id.to_def_id()));
        let module = self.r.get_module(module_id.to_def_id()).unwrap();
        let resolutions = self.r.resolutions(module);

        for (_, name_resolution) in resolutions.borrow().iter() {
            if let Some(mut binding) = name_resolution.borrow().binding() && !binding.is_ambiguity() {
                // Set the given effective visibility level to `Level::Direct` and
                // sets the rest of the `use` chain to `Level::Reexported` until
                // we hit the actual exported item.
                let mut parent_id = ParentId::Def(module_id);
                while let NameBindingKind::Import { binding: nested_binding, .. } = binding.kind {
                    let binding_id = ImportId::new_unchecked(binding);
                    self.update_import(binding_id, parent_id);

                    parent_id = ParentId::Import(binding_id);
                    binding = nested_binding;
                }

                if let Some(def_id) = binding.res().opt_def_id().and_then(|id| id.as_local()) {
                    self.update_def(def_id, binding.vis.expect_local(), parent_id);
                }
            }
        }
    }

    fn effective_vis(&self, parent_id: ParentId<'a>) -> Option<EffectiveVisibility> {
        match parent_id {
            ParentId::Def(def_id) => self.r.effective_visibilities.effective_vis(def_id),
            ParentId::Import(binding) => self.import_effective_visibilities.effective_vis(binding),
        }
        .copied()
    }

    /// The update is guaranteed to not change the table and we can skip it.
    fn is_noop_update(
        &self,
        parent_id: ParentId<'a>,
        nominal_vis: Visibility,
        default_vis: Visibility,
    ) -> bool {
        nominal_vis == default_vis
            || match parent_id {
                ParentId::Def(def_id) => self.r.visibilities[&def_id],
                ParentId::Import(binding) => binding.vis.expect_local(),
            } == default_vis
    }

    fn update_import(&mut self, binding: ImportId<'a>, parent_id: ParentId<'a>) {
        let NameBindingKind::Import { import, .. } = binding.kind else { unreachable!() };
        let nominal_vis = binding.vis.expect_local();
        let default_vis = Visibility::Restricted(
            import
                .id()
                .map(|id| self.nearest_normal_mod(self.r.local_def_id(id)))
                .unwrap_or(CRATE_DEF_ID),
        );
        if self.is_noop_update(parent_id, nominal_vis, default_vis) {
            return;
        }
        self.changed |= self.import_effective_visibilities.update(
            binding,
            nominal_vis,
            default_vis,
            self.effective_vis(parent_id),
            parent_id.level(),
            ResolverTree(&self.r.definitions, &self.r.crate_loader),
        );
    }

    fn update_def(&mut self, def_id: LocalDefId, nominal_vis: Visibility, parent_id: ParentId<'a>) {
        let default_vis = Visibility::Restricted(self.nearest_normal_mod(def_id));
        if self.is_noop_update(parent_id, nominal_vis, default_vis) {
            return;
        }
        self.changed |= self.r.effective_visibilities.update(
            def_id,
            nominal_vis,
            if def_id == CRATE_DEF_ID { Visibility::Public } else { default_vis },
            self.effective_vis(parent_id),
            parent_id.level(),
            ResolverTree(&self.r.definitions, &self.r.crate_loader),
        );
    }

    fn update(&mut self, def_id: LocalDefId, parent_id: LocalDefId) {
        self.update_def(def_id, self.r.visibilities[&def_id], ParentId::Def(parent_id));
    }
}

impl<'r, 'ast> Visitor<'ast> for EffectiveVisibilitiesVisitor<'ast, 'r> {
    fn visit_item(&mut self, item: &'ast ast::Item) {
        let def_id = self.r.local_def_id(item.id);
        // Update effective visibilities of nested items.
        // If it's a mod, also make the visitor walk all of its items
        match item.kind {
            // Resolved in rustc_privacy when types are available
            ast::ItemKind::Impl(..) => return,

            // Should be unreachable at this stage
            ast::ItemKind::MacCall(..) => panic!(
                "ast::ItemKind::MacCall encountered, this should not anymore appear at this stage"
            ),

            ast::ItemKind::Mod(..) => {
                self.set_bindings_effective_visibilities(def_id);
                visit::walk_item(self, item);
            }

            ast::ItemKind::Enum(EnumDef { ref variants }, _) => {
                self.set_bindings_effective_visibilities(def_id);
                for variant in variants {
                    let variant_def_id = self.r.local_def_id(variant.id);
                    for field in variant.data.fields() {
                        self.update(self.r.local_def_id(field.id), variant_def_id);
                    }
                }
            }

            ast::ItemKind::Struct(ref def, _) | ast::ItemKind::Union(ref def, _) => {
                for field in def.fields() {
                    self.update(self.r.local_def_id(field.id), def_id);
                }
            }

            ast::ItemKind::Trait(..) => {
                self.set_bindings_effective_visibilities(def_id);
            }

            ast::ItemKind::ExternCrate(..)
            | ast::ItemKind::Use(..)
            | ast::ItemKind::Static(..)
            | ast::ItemKind::Const(..)
            | ast::ItemKind::GlobalAsm(..)
            | ast::ItemKind::TyAlias(..)
            | ast::ItemKind::TraitAlias(..)
            | ast::ItemKind::MacroDef(..)
            | ast::ItemKind::ForeignMod(..)
            | ast::ItemKind::Fn(..) => return,
        }
    }
}
