//! In rust, it is possible to have a value, a type and a macro with the same
//! name without conflicts.
//!
//! `PerNs` (per namespace) captures this.

use crate::{
    item_scope::{ImportId, ImportOrExternCrate, ItemInNs},
    visibility::Visibility,
    MacroId, ModuleDefId,
};

#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
pub enum Namespace {
    Types,
    Values,
    Macros,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct PerNs {
    pub types: Option<(ModuleDefId, Visibility, Option<ImportOrExternCrate>)>,
    pub values: Option<(ModuleDefId, Visibility, Option<ImportId>)>,
    pub macros: Option<(MacroId, Visibility, Option<ImportId>)>,
}

impl Default for PerNs {
    fn default() -> Self {
        PerNs { types: None, values: None, macros: None }
    }
}

impl PerNs {
    pub fn none() -> PerNs {
        PerNs { types: None, values: None, macros: None }
    }

    pub fn values(t: ModuleDefId, v: Visibility, i: Option<ImportId>) -> PerNs {
        PerNs { types: None, values: Some((t, v, i)), macros: None }
    }

    pub fn types(t: ModuleDefId, v: Visibility, i: Option<ImportOrExternCrate>) -> PerNs {
        PerNs { types: Some((t, v, i)), values: None, macros: None }
    }

    pub fn both(
        types: ModuleDefId,
        values: ModuleDefId,
        v: Visibility,
        i: Option<ImportOrExternCrate>,
    ) -> PerNs {
        PerNs {
            types: Some((types, v, i)),
            values: Some((values, v, i.and_then(ImportOrExternCrate::into_import))),
            macros: None,
        }
    }

    pub fn macros(macro_: MacroId, v: Visibility, i: Option<ImportId>) -> PerNs {
        PerNs { types: None, values: None, macros: Some((macro_, v, i)) }
    }

    pub fn is_none(&self) -> bool {
        self.types.is_none() && self.values.is_none() && self.macros.is_none()
    }

    pub fn is_full(&self) -> bool {
        self.types.is_some() && self.values.is_some() && self.macros.is_some()
    }

    pub fn take_types(self) -> Option<ModuleDefId> {
        self.types.map(|it| it.0)
    }

    pub fn take_types_full(self) -> Option<(ModuleDefId, Visibility, Option<ImportOrExternCrate>)> {
        self.types
    }

    pub fn take_values(self) -> Option<ModuleDefId> {
        self.values.map(|it| it.0)
    }

    pub fn take_values_import(self) -> Option<(ModuleDefId, Option<ImportId>)> {
        self.values.map(|it| (it.0, it.2))
    }

    pub fn take_macros(self) -> Option<MacroId> {
        self.macros.map(|it| it.0)
    }

    pub fn take_macros_import(self) -> Option<(MacroId, Option<ImportId>)> {
        self.macros.map(|it| (it.0, it.2))
    }

    pub fn filter_visibility(self, mut f: impl FnMut(Visibility) -> bool) -> PerNs {
        let _p = profile::span("PerNs::filter_visibility");
        PerNs {
            types: self.types.filter(|&(_, v, _)| f(v)),
            values: self.values.filter(|&(_, v, _)| f(v)),
            macros: self.macros.filter(|&(_, v, _)| f(v)),
        }
    }

    pub fn with_visibility(self, vis: Visibility) -> PerNs {
        PerNs {
            types: self.types.map(|(it, _, c)| (it, vis, c)),
            values: self.values.map(|(it, _, c)| (it, vis, c)),
            macros: self.macros.map(|(it, _, import)| (it, vis, import)),
        }
    }

    pub fn or(self, other: PerNs) -> PerNs {
        PerNs {
            types: self.types.or(other.types),
            values: self.values.or(other.values),
            macros: self.macros.or(other.macros),
        }
    }

    pub fn or_else(self, f: impl FnOnce() -> PerNs) -> PerNs {
        if self.is_full() {
            self
        } else {
            self.or(f())
        }
    }

    pub fn iter_items(self) -> impl Iterator<Item = (ItemInNs, Option<ImportOrExternCrate>)> {
        let _p = profile::span("PerNs::iter_items");
        self.types
            .map(|it| (ItemInNs::Types(it.0), it.2))
            .into_iter()
            .chain(
                self.values
                    .map(|it| (ItemInNs::Values(it.0), it.2.map(ImportOrExternCrate::Import)))
                    .into_iter(),
            )
            .chain(
                self.macros
                    .map(|it| (ItemInNs::Macros(it.0), it.2.map(ImportOrExternCrate::Import)))
                    .into_iter(),
            )
    }
}
