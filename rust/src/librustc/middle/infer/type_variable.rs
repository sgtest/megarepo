// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

pub use self::RelationDir::*;
use self::TypeVariableValue::*;
use self::UndoEntry::*;
use middle::ty::{self, Ty};
use syntax::ast::DefId;
use syntax::codemap::Span;

use std::cmp::min;
use std::marker::PhantomData;
use std::mem;
use std::u32;
use rustc_data_structures::snapshot_vec as sv;

pub struct TypeVariableTable<'tcx> {
    values: sv::SnapshotVec<Delegate<'tcx>>,
}

struct TypeVariableData<'tcx> {
    value: TypeVariableValue<'tcx>,
    diverging: bool
}

enum TypeVariableValue<'tcx> {
    Known(Ty<'tcx>),
    Bounded {
        relations: Vec<Relation>,
        default: Option<Default<'tcx>>
    }
}

// We will use this to store the required information to recapitulate what happened when
// an error occurs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Default<'tcx> {
    pub ty: Ty<'tcx>,
    /// The span where the default was incurred
    pub origin_span: Span,
    /// The definition that the default originates from
    pub def_id: DefId
}

pub struct Snapshot {
    snapshot: sv::Snapshot
}

enum UndoEntry<'tcx> {
    // The type of the var was specified.
    SpecifyVar(ty::TyVid, Vec<Relation>, Option<Default<'tcx>>),
    Relate(ty::TyVid, ty::TyVid),
}

struct Delegate<'tcx>(PhantomData<&'tcx ()>);

type Relation = (RelationDir, ty::TyVid);

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum RelationDir {
    SubtypeOf, SupertypeOf, EqTo, BiTo
}

impl RelationDir {
    fn opposite(self) -> RelationDir {
        match self {
            SubtypeOf => SupertypeOf,
            SupertypeOf => SubtypeOf,
            EqTo => EqTo,
            BiTo => BiTo,
        }
    }
}

impl<'tcx> TypeVariableTable<'tcx> {
    pub fn new() -> TypeVariableTable<'tcx> {
        TypeVariableTable { values: sv::SnapshotVec::new() }
    }

    fn relations<'a>(&'a mut self, a: ty::TyVid) -> &'a mut Vec<Relation> {
        relations(self.values.get_mut(a.index as usize))
    }

    pub fn default(&self, vid: ty::TyVid) -> Option<Default<'tcx>> {
        match &self.values.get(vid.index as usize).value {
            &Known(_) => None,
            &Bounded { ref default, .. } => default.clone()
        }
    }

    pub fn var_diverges<'a>(&'a self, vid: ty::TyVid) -> bool {
        self.values.get(vid.index as usize).diverging
    }

    /// Records that `a <: b`, `a :> b`, or `a == b`, depending on `dir`.
    ///
    /// Precondition: neither `a` nor `b` are known.
    pub fn relate_vars(&mut self, a: ty::TyVid, dir: RelationDir, b: ty::TyVid) {
        if a != b {
            self.relations(a).push((dir, b));
            self.relations(b).push((dir.opposite(), a));
            self.values.record(Relate(a, b));
        }
    }

    /// Instantiates `vid` with the type `ty` and then pushes an entry onto `stack` for each of the
    /// relations of `vid` to other variables. The relations will have the form `(ty, dir, vid1)`
    /// where `vid1` is some other variable id.
    pub fn instantiate_and_push(
        &mut self,
        vid: ty::TyVid,
        ty: Ty<'tcx>,
        stack: &mut Vec<(Ty<'tcx>, RelationDir, ty::TyVid)>)
    {
        let old_value = {
            let value_ptr = &mut self.values.get_mut(vid.index as usize).value;
            mem::replace(value_ptr, Known(ty))
        };

        let (relations, default) = match old_value {
            Bounded { relations, default } => (relations, default),
            Known(_) => panic!("Asked to instantiate variable that is \
                               already instantiated")
        };

        for &(dir, vid) in &relations {
            stack.push((ty, dir, vid));
        }

        self.values.record(SpecifyVar(vid, relations, default));
    }

    pub fn new_var(&mut self,
                   diverging: bool,
                   default: Option<Default<'tcx>>) -> ty::TyVid {
        let index = self.values.push(TypeVariableData {
            value: Bounded { relations: vec![], default: default },
            diverging: diverging
        });
        ty::TyVid { index: index as u32 }
    }

    pub fn probe(&self, vid: ty::TyVid) -> Option<Ty<'tcx>> {
        match self.values.get(vid.index as usize).value {
            Bounded { .. } => None,
            Known(t) => Some(t)
        }
    }

    pub fn replace_if_possible(&self, t: Ty<'tcx>) -> Ty<'tcx> {
        match t.sty {
            ty::TyInfer(ty::TyVar(v)) => {
                match self.probe(v) {
                    None => t,
                    Some(u) => u
                }
            }
            _ => t,
        }
    }

    pub fn snapshot(&mut self) -> Snapshot {
        Snapshot { snapshot: self.values.start_snapshot() }
    }

    pub fn rollback_to(&mut self, s: Snapshot) {
        self.values.rollback_to(s.snapshot);
    }

    pub fn commit(&mut self, s: Snapshot) {
        self.values.commit(s.snapshot);
    }

    pub fn types_escaping_snapshot(&self, s: &Snapshot) -> Vec<Ty<'tcx>> {
        /*!
         * Find the set of type variables that existed *before* `s`
         * but which have only been unified since `s` started, and
         * return the types with which they were unified. So if we had
         * a type variable `V0`, then we started the snapshot, then we
         * created a type variable `V1`, unifed `V0` with `T0`, and
         * unified `V1` with `T1`, this function would return `{T0}`.
         */

        let mut new_elem_threshold = u32::MAX;
        let mut escaping_types = Vec::new();
        let actions_since_snapshot = self.values.actions_since_snapshot(&s.snapshot);
        debug!("actions_since_snapshot.len() = {}", actions_since_snapshot.len());
        for action in actions_since_snapshot {
            match *action {
                sv::UndoLog::NewElem(index) => {
                    // if any new variables were created during the
                    // snapshot, remember the lower index (which will
                    // always be the first one we see). Note that this
                    // action must precede those variables being
                    // specified.
                    new_elem_threshold = min(new_elem_threshold, index as u32);
                    debug!("NewElem({}) new_elem_threshold={}", index, new_elem_threshold);
                }

                sv::UndoLog::Other(SpecifyVar(vid, _, _)) => {
                    if vid.index < new_elem_threshold {
                        // quick check to see if this variable was
                        // created since the snapshot started or not.
                        let escaping_type = self.probe(vid).unwrap();
                        escaping_types.push(escaping_type);
                    }
                    debug!("SpecifyVar({:?}) new_elem_threshold={}", vid, new_elem_threshold);
                }

                _ => { }
            }
        }

        escaping_types
    }

    pub fn unsolved_variables(&self) -> Vec<ty::TyVid> {
        self.values
            .iter()
            .enumerate()
            .filter_map(|(i, value)| match &value.value {
                &TypeVariableValue::Known(_) => None,
                &TypeVariableValue::Bounded { .. } => Some(ty::TyVid { index: i as u32 })
            })
            .collect()
    }
}

impl<'tcx> sv::SnapshotVecDelegate for Delegate<'tcx> {
    type Value = TypeVariableData<'tcx>;
    type Undo = UndoEntry<'tcx>;

    fn reverse(values: &mut Vec<TypeVariableData<'tcx>>, action: UndoEntry<'tcx>) {
        match action {
            SpecifyVar(vid, relations, default) => {
                values[vid.index as usize].value = Bounded {
                    relations: relations,
                    default: default
                };
            }

            Relate(a, b) => {
                relations(&mut (*values)[a.index as usize]).pop();
                relations(&mut (*values)[b.index as usize]).pop();
            }
        }
    }
}

fn relations<'a>(v: &'a mut TypeVariableData) -> &'a mut Vec<Relation> {
    match v.value {
        Known(_) => panic!("var_sub_var: variable is known"),
        Bounded { ref mut relations, .. } => relations
    }
}
