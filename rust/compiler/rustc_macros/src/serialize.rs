use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::parse_quote;
use syn::spanned::Spanned;

pub fn type_decodable_derive(mut s: synstructure::Structure<'_>) -> proc_macro2::TokenStream {
    let decoder_ty = quote! { __D };
    if !s.ast().generics.lifetimes().any(|lt| lt.lifetime.ident == "tcx") {
        s.add_impl_generic(parse_quote! { 'tcx });
    }
    s.add_impl_generic(parse_quote! {#decoder_ty: ::rustc_middle::ty::codec::TyDecoder<'tcx>});
    s.add_bounds(synstructure::AddBounds::Generics);

    decodable_body(s, decoder_ty)
}

pub fn meta_decodable_derive(mut s: synstructure::Structure<'_>) -> proc_macro2::TokenStream {
    if !s.ast().generics.lifetimes().any(|lt| lt.lifetime.ident == "tcx") {
        s.add_impl_generic(parse_quote! { 'tcx });
    }
    s.add_impl_generic(parse_quote! { '__a });
    let decoder_ty = quote! { DecodeContext<'__a, 'tcx> };
    s.add_bounds(synstructure::AddBounds::Generics);

    decodable_body(s, decoder_ty)
}

pub fn decodable_derive(mut s: synstructure::Structure<'_>) -> proc_macro2::TokenStream {
    let decoder_ty = quote! { __D };
    s.add_impl_generic(parse_quote! {#decoder_ty: ::rustc_serialize::Decoder});
    s.add_bounds(synstructure::AddBounds::Generics);

    decodable_body(s, decoder_ty)
}

fn decodable_body(
    s: synstructure::Structure<'_>,
    decoder_ty: TokenStream,
) -> proc_macro2::TokenStream {
    if let syn::Data::Union(_) = s.ast().data {
        panic!("cannot derive on union")
    }
    let ty_name = s.ast().ident.to_string();
    let decode_body = match s.variants() {
        [vi] => vi.construct(|field, _index| decode_field(field)),
        variants => {
            let match_inner: TokenStream = variants
                .iter()
                .enumerate()
                .map(|(idx, vi)| {
                    let construct = vi.construct(|field, _index| decode_field(field));
                    quote! { #idx => { #construct } }
                })
                .collect();
            let message = format!(
                "invalid enum variant tag while decoding `{}`, expected 0..{}",
                ty_name,
                variants.len()
            );
            quote! {
                match ::rustc_serialize::Decoder::read_usize(__decoder) {
                    #match_inner
                    _ => panic!(#message),
                }
            }
        }
    };

    s.bound_impl(
        quote!(::rustc_serialize::Decodable<#decoder_ty>),
        quote! {
            fn decode(__decoder: &mut #decoder_ty) -> Self {
                #decode_body
            }
        },
    )
}

fn decode_field(field: &syn::Field) -> proc_macro2::TokenStream {
    let field_span = field.ident.as_ref().map_or(field.ty.span(), |ident| ident.span());

    let decode_inner_method = if let syn::Type::Reference(_) = field.ty {
        quote! { ::rustc_middle::ty::codec::RefDecodable::decode }
    } else {
        quote! { ::rustc_serialize::Decodable::decode }
    };
    let __decoder = quote! { __decoder };
    // Use the span of the field for the method call, so
    // that backtraces will point to the field.
    quote_spanned! {field_span=> #decode_inner_method(#__decoder) }
}

pub fn type_encodable_derive(mut s: synstructure::Structure<'_>) -> proc_macro2::TokenStream {
    if !s.ast().generics.lifetimes().any(|lt| lt.lifetime.ident == "tcx") {
        s.add_impl_generic(parse_quote! {'tcx});
    }
    let encoder_ty = quote! { __E };
    s.add_impl_generic(parse_quote! {#encoder_ty: ::rustc_middle::ty::codec::TyEncoder<'tcx>});
    s.add_bounds(synstructure::AddBounds::Generics);

    encodable_body(s, encoder_ty, false)
}

pub fn meta_encodable_derive(mut s: synstructure::Structure<'_>) -> proc_macro2::TokenStream {
    if !s.ast().generics.lifetimes().any(|lt| lt.lifetime.ident == "tcx") {
        s.add_impl_generic(parse_quote! {'tcx});
    }
    s.add_impl_generic(parse_quote! { '__a });
    let encoder_ty = quote! { EncodeContext<'__a, 'tcx> };
    s.add_bounds(synstructure::AddBounds::Generics);

    encodable_body(s, encoder_ty, true)
}

pub fn encodable_derive(mut s: synstructure::Structure<'_>) -> proc_macro2::TokenStream {
    let encoder_ty = quote! { __E };
    s.add_impl_generic(parse_quote! { #encoder_ty: ::rustc_serialize::Encoder});
    s.add_bounds(synstructure::AddBounds::Generics);

    encodable_body(s, encoder_ty, false)
}

fn encodable_body(
    mut s: synstructure::Structure<'_>,
    encoder_ty: TokenStream,
    allow_unreachable_code: bool,
) -> proc_macro2::TokenStream {
    if let syn::Data::Union(_) = s.ast().data {
        panic!("cannot derive on union")
    }

    s.bind_with(|binding| {
        // Handle the lack of a blanket reference impl.
        if let syn::Type::Reference(_) = binding.ast().ty {
            synstructure::BindStyle::Move
        } else {
            synstructure::BindStyle::Ref
        }
    });

    let encode_body = match s.variants() {
        [_] => {
            let mut field_idx = 0usize;
            let encode_inner = s.each_variant(|vi| {
                vi.bindings()
                    .iter()
                    .map(|binding| {
                        let bind_ident = &binding.binding;
                        let field_name = binding
                            .ast()
                            .ident
                            .as_ref()
                            .map_or_else(|| field_idx.to_string(), |i| i.to_string());
                        let first = field_idx == 0;
                        let result = quote! {
                            match ::rustc_serialize::Encoder::emit_struct_field(
                                __encoder,
                                #field_name,
                                #first,
                                |__encoder|
                                ::rustc_serialize::Encodable::<#encoder_ty>::encode(#bind_ident, __encoder),
                            ) {
                                ::std::result::Result::Ok(()) => (),
                                ::std::result::Result::Err(__err)
                                    => return ::std::result::Result::Err(__err),
                            }
                        };
                        field_idx += 1;
                        result
                    })
                    .collect::<TokenStream>()
            });
            let no_fields = field_idx == 0;
            quote! {
                ::rustc_serialize::Encoder::emit_struct(__encoder, #no_fields, |__encoder| {
                    ::std::result::Result::Ok(match *self { #encode_inner })
                })
            }
        }
        _ => {
            let mut variant_idx = 0usize;
            let encode_inner = s.each_variant(|vi| {
                let variant_name = vi.ast().ident.to_string();
                let mut field_idx = 0usize;

                let encode_fields: TokenStream = vi
                    .bindings()
                    .iter()
                    .map(|binding| {
                        let bind_ident = &binding.binding;
                        let first = field_idx == 0;
                        let result = quote! {
                            match ::rustc_serialize::Encoder::emit_enum_variant_arg(
                                __encoder,
                                #first,
                                |__encoder|
                                ::rustc_serialize::Encodable::<#encoder_ty>::encode(#bind_ident, __encoder),
                            ) {
                                ::std::result::Result::Ok(()) => (),
                                ::std::result::Result::Err(__err)
                                    => return ::std::result::Result::Err(__err),
                            }
                        };
                        field_idx += 1;
                        result
                    })
                    .collect();

                let result = if field_idx != 0 {
                    quote! {
                        ::rustc_serialize::Encoder::emit_enum_variant(
                            __encoder,
                            #variant_name,
                            #variant_idx,
                            #field_idx,
                            |__encoder| { ::std::result::Result::Ok({ #encode_fields }) }
                        )
                    }
                } else {
                    quote! {
                        ::rustc_serialize::Encoder::emit_fieldless_enum_variant::<#variant_idx>(
                            __encoder,
                            #variant_name,
                        )
                    }
                };
                variant_idx += 1;
                result
            });
            quote! {
                ::rustc_serialize::Encoder::emit_enum(__encoder, |__encoder| {
                    match *self {
                        #encode_inner
                    }
                })
            }
        }
    };

    let lints = if allow_unreachable_code {
        quote! { #![allow(unreachable_code)] }
    } else {
        quote! {}
    };

    s.bound_impl(
        quote!(::rustc_serialize::Encodable<#encoder_ty>),
        quote! {
            fn encode(
                &self,
                __encoder: &mut #encoder_ty,
            ) -> ::std::result::Result<(), <#encoder_ty as ::rustc_serialize::Encoder>::Error> {
                #lints
                #encode_body
            }
        },
    )
}
