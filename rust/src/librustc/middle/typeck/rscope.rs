// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::prelude::*;

use middle::ty;

use core::result::Result;
use core::result;
use syntax::ast;
use syntax::codemap::span;
use syntax::opt_vec::OptVec;
use syntax::opt_vec;
use syntax::parse::token::special_idents;

pub struct RegionError {
    msg: ~str,
    replacement: ty::Region
}

pub trait region_scope {
    fn anon_region(&self, span: span) -> Result<ty::Region, RegionError>;
    fn self_region(&self, span: span) -> Result<ty::Region, RegionError>;
    fn named_region(&self, span: span, id: ast::ident)
                      -> Result<ty::Region, RegionError>;
}

pub enum empty_rscope { empty_rscope }
impl region_scope for empty_rscope {
    fn anon_region(&self, _span: span) -> Result<ty::Region, RegionError> {
        result::Err(RegionError {
            msg: ~"only 'static is allowed here",
            replacement: ty::re_static
        })
    }
    fn self_region(&self, _span: span) -> Result<ty::Region, RegionError> {
        self.anon_region(_span)
    }
    fn named_region(&self, _span: span, _id: ast::ident)
        -> Result<ty::Region, RegionError>
    {
        self.anon_region(_span)
    }
}

pub struct RegionParamNames(OptVec<ast::ident>);

impl RegionParamNames {
    fn has_self(&self) -> bool {
        self.has_ident(special_idents::self_)
    }

    fn has_ident(&self, ident: ast::ident) -> bool {
        for self.each |region_param_name| {
            if *region_param_name == ident {
                return true;
            }
        }
        false
    }

    pub fn add_generics(&mut self, generics: &ast::Generics) {
        match generics.lifetimes {
            opt_vec::Empty => {}
            opt_vec::Vec(ref new_lifetimes) => {
                match **self {
                    opt_vec::Empty => {
                        *self = RegionParamNames(
                            opt_vec::Vec(new_lifetimes.map(|lt| lt.ident)));
                    }
                    opt_vec::Vec(ref mut existing_lifetimes) => {
                        for new_lifetimes.each |new_lifetime| {
                            existing_lifetimes.push(new_lifetime.ident);
                        }
                    }
                }
            }
        }
    }

    // Convenience function to produce the error for an unresolved name. The
    // optional argument specifies a custom replacement.
    pub fn undeclared_name(custom_replacement: Option<ty::Region>)
                        -> Result<ty::Region, RegionError> {
        let replacement = match custom_replacement {
            None => ty::re_bound(ty::br_self),
            Some(custom_replacement) => custom_replacement
        };
        Err(RegionError {
            msg: ~"this lifetime must be declared",
            replacement: replacement
        })
    }

    pub fn from_generics(generics: &ast::Generics) -> RegionParamNames {
        match generics.lifetimes {
            opt_vec::Empty => RegionParamNames(opt_vec::Empty),
            opt_vec::Vec(ref lifetimes) => {
                RegionParamNames(opt_vec::Vec(lifetimes.map(|lt| lt.ident)))
            }
        }
    }

    pub fn from_lifetimes(lifetimes: &opt_vec::OptVec<ast::Lifetime>)
                       -> RegionParamNames {
        match *lifetimes {
            opt_vec::Empty => RegionParamNames::new(),
            opt_vec::Vec(ref v) => {
                RegionParamNames(opt_vec::Vec(v.map(|lt| lt.ident)))
            }
        }
    }

    fn new() -> RegionParamNames {
        RegionParamNames(opt_vec::Empty)
    }
}

struct RegionParameterization {
    variance: ty::region_variance,
    region_param_names: RegionParamNames,
}

impl RegionParameterization {
    pub fn from_variance_and_generics(variance: Option<ty::region_variance>,
                                      generics: &ast::Generics)
                                   -> Option<RegionParameterization> {
        match variance {
            None => None,
            Some(variance) => {
                Some(RegionParameterization {
                    variance: variance,
                    region_param_names:
                        RegionParamNames::from_generics(generics),
                })
            }
        }
    }
}

pub struct MethodRscope {
    self_ty: ast::self_ty_,
    variance: Option<ty::region_variance>,
    region_param_names: RegionParamNames,
}

impl MethodRscope {
    // `generics` here refers to the generics of the outer item (impl or
    // trait).
    pub fn new(self_ty: ast::self_ty_,
               variance: Option<ty::region_variance>,
               rcvr_generics: &ast::Generics)
            -> MethodRscope {
        let mut region_param_names =
            RegionParamNames::from_generics(rcvr_generics);
        MethodRscope {
            self_ty: self_ty,
            variance: variance,
            region_param_names: region_param_names
        }
    }

    pub fn region_param_names(&self) -> RegionParamNames {
        copy self.region_param_names
    }
}

impl region_scope for MethodRscope {
    fn anon_region(&self, _span: span) -> Result<ty::Region, RegionError> {
        result::Err(RegionError {
            msg: ~"anonymous lifetimes are not permitted here",
            replacement: ty::re_bound(ty::br_self)
        })
    }
    fn self_region(&self, _span: span) -> Result<ty::Region, RegionError> {
        assert!(self.variance.is_some() || self.self_ty.is_borrowed());
        match self.variance {
            None => {}  // must be borrowed self, so this is OK
            Some(_) => {
                if !self.self_ty.is_borrowed() &&
                        !self.region_param_names.has_self() {
                    return Err(RegionError {
                        msg: ~"the `self` lifetime must be declared",
                        replacement: ty::re_bound(ty::br_self)
                    })
                }
            }
        }
        result::Ok(ty::re_bound(ty::br_self))
    }
    fn named_region(&self, span: span, id: ast::ident)
                      -> Result<ty::Region, RegionError> {
        if !self.region_param_names.has_ident(id) {
            return RegionParamNames::undeclared_name(None);
        }
        do empty_rscope.named_region(span, id).chain_err |_e| {
            result::Err(RegionError {
                msg: ~"lifetime is not in scope",
                replacement: ty::re_bound(ty::br_self)
            })
        }
    }
}

pub struct type_rscope(Option<RegionParameterization>);

impl type_rscope {
    priv fn replacement(&self) -> ty::Region {
        if self.is_some() {
            ty::re_bound(ty::br_self)
        } else {
            ty::re_static
        }
    }
}
impl region_scope for type_rscope {
    fn anon_region(&self, _span: span) -> Result<ty::Region, RegionError> {
        result::Err(RegionError {
            msg: ~"anonymous lifetimes are not permitted here",
            replacement: self.replacement()
        })
    }
    fn self_region(&self, _span: span) -> Result<ty::Region, RegionError> {
        match **self {
            None => {
                // if the self region is used, region parameterization should
                // have inferred that this type is RP
                fail!(~"region parameterization should have inferred that \
                        this type is RP");
            }
            Some(ref region_parameterization) => {
                if !region_parameterization.region_param_names.has_self() {
                    return Err(RegionError {
                        msg: ~"the `self` lifetime must be declared",
                        replacement: ty::re_bound(ty::br_self)
                    })
                }
            }
        }
        result::Ok(ty::re_bound(ty::br_self))
    }
    fn named_region(&self, span: span, id: ast::ident)
                      -> Result<ty::Region, RegionError> {
        do empty_rscope.named_region(span, id).chain_err |_e| {
            result::Err(RegionError {
                msg: ~"only 'self is allowed as part of a type declaration",
                replacement: self.replacement()
            })
        }
    }
}

pub fn bound_self_region(rp: Option<ty::region_variance>)
                      -> Option<ty::Region> {
    match rp {
      Some(_) => Some(ty::re_bound(ty::br_self)),
      None => None
    }
}

pub struct binding_rscope {
    base: @region_scope,
    anon_bindings: @mut uint,
    region_param_names: RegionParamNames,
}

pub fn in_binding_rscope<RS:region_scope + Copy + Durable>(
        self: &RS,
        +region_param_names: RegionParamNames)
     -> binding_rscope {
    let base = @copy *self;
    let base = base as @region_scope;
    binding_rscope {
        base: base,
        anon_bindings: @mut 0,
        region_param_names: region_param_names,
    }
}

impl region_scope for binding_rscope {
    fn anon_region(&self, _span: span) -> Result<ty::Region, RegionError> {
        let idx = *self.anon_bindings;
        *self.anon_bindings += 1;
        result::Ok(ty::re_bound(ty::br_anon(idx)))
    }
    fn self_region(&self, span: span) -> Result<ty::Region, RegionError> {
        self.base.self_region(span)
    }
    fn named_region(&self,
                    span: span,
                    id: ast::ident) -> Result<ty::Region, RegionError>
    {
        do self.base.named_region(span, id).chain_err |_e| {
            let result = ty::re_bound(ty::br_named(id));
            if self.region_param_names.has_ident(id) {
                result::Ok(result)
            } else {
                RegionParamNames::undeclared_name(Some(result))
            }
        }
    }
}
