extern crate proc_macro;

use lazy_static::lazy_static;
use proc_macro2::TokenStream;
use proc_macro_error::{abort_call_site, proc_macro_error};
use quote::quote;
use syn::{spanned::Spanned, *};

mod ty;

lazy_static! {
    pub(crate) static ref UNNAMED_FIELDS: Vec<&'static str> =
        vec!["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"];
}

#[proc_macro_derive(Cborize, attributes(cbor))]
#[proc_macro_error]
pub fn cborize_type(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input: DeriveInput = syn::parse(input).unwrap();
    let gen = match &input.data {
        Data::Struct(_) => impl_cborize_struct(&input, false),
        Data::Enum(_) => impl_cborize_enum(&input, false),
        Data::Union(_) => abort_call_site!("cannot derive Cborize for union"),
    };
    gen.into()
}

#[proc_macro_derive(LocalCborize, attributes(cbor))]
#[proc_macro_error]
pub fn local_cborize_type(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input: DeriveInput = syn::parse(input).unwrap();
    let gen = match &input.data {
        Data::Struct(_) => impl_cborize_struct(&input, true),
        Data::Enum(_) => impl_cborize_enum(&input, true),
        Data::Union(_) => {
            abort_call_site!("cannot derive LocalCborize for union")
        }
    };
    gen.into()
}

fn impl_cborize_struct(input: &DeriveInput, crate_local: bool) -> TokenStream {
    let name = &input.ident;
    let generics = no_default_generics(input);

    let mut ts = TokenStream::new();
    match &input.data {
        Data::Struct(ast) => {
            ts.extend(from_struct_to_cbor(
                name,
                &generics,
                &ast.fields,
                crate_local,
            ));
            ts.extend(from_cbor_to_struct(
                name,
                &generics,
                &ast.fields,
                crate_local,
            ));
            ts
        }
        _ => unreachable!(),
    }
}

fn from_struct_to_cbor(
    name: &Ident,
    generics: &Generics,
    fields: &Fields,
    crate_local: bool,
) -> TokenStream {
    let id_declr = let_id(name, generics);
    let croot = get_root_crate(crate_local);
    let preamble = quote! {
        let val: #croot::cbor::Cbor = {
            #id_declr;
            #croot::cbor::Tag::from_identifier(id).into()
        };
        items.push(val);
    };

    let token_fields = match fields {
        Fields::Unit => quote! {},
        Fields::Named(fields) => named_fields_to_cbor(fields, croot.clone()),
        Fields::Unnamed(_) => {
            abort_call_site!("unnamed struct not supported for Cborize {}", name)
        }
    };

    let mut where_clause = match &generics.where_clause {
        Some(where_clause) => quote! { #where_clause },
        None => quote! { where },
    };
    for param in generics.params.iter() {
        let type_var = match param {
            GenericParam::Type(param) => &param.ident,
            _ => abort_call_site!("only type parameter are supported"),
        };
        where_clause.extend(quote! { #type_var: #croot::cbor::IntoCbor, });
    }

    quote! {
        impl#generics #croot::cbor::IntoCbor for #name#generics #where_clause {
            fn into_cbor(self) -> #croot::Result<#croot::cbor::Cbor> {
                let value = self;
                let mut items: Vec<#croot::cbor::Cbor> = Vec::default();

                #preamble
                #token_fields;

                Ok(items.into_cbor()?)
            }
        }
    }
}

fn from_cbor_to_struct(
    name: &Ident,
    generics: &Generics,
    fields: &Fields,
    crate_local: bool,
) -> TokenStream {
    let name_lit = name.to_string();
    let croot = get_root_crate(crate_local);
    let n_fields = match fields {
        Fields::Unit => 0,
        Fields::Named(fields) => fields.named.len(),
        Fields::Unnamed(_) => {
            abort_call_site!("unnamed struct not supported for Cborize {}", name)
        }
    };

    let id_declr = let_id(name, generics);
    let preamble = quote! {
        // validate the cbor msg for this type.
        if items.len() == 0 {
            #croot::err_at!(FailConvert, msg: "empty msg for {}", #name_lit)?;
        }
        let data_id = items.remove(0);
        let type_id: #croot::cbor::Cbor = {
            #id_declr;
            #croot::cbor::Tag::from_identifier(id).into()
        };
        if data_id != type_id {
            #croot::err_at!(FailConvert, msg: "bad id for {}", #name_lit)?;
        }
        if #n_fields != items.len() {
            #croot::err_at!(FailConvert, msg: "bad arity {} {}", #n_fields, items.len())?;
        }
    };

    let token_fields = match fields {
        Fields::Unit => quote! {},
        Fields::Named(fields) => {
            let token_fields = cbor_to_named_fields(fields, croot.clone());
            quote! { { #token_fields } }
        }
        Fields::Unnamed(_) => {
            abort_call_site!("unnamed struct not supported for Cborize {}", name)
        }
    };

    let mut where_clause = match &generics.where_clause {
        Some(where_clause) => quote! { #where_clause },
        None => quote! { where },
    };
    for param in generics.params.iter() {
        let type_var = match param {
            GenericParam::Type(param) => &param.ident,
            _ => abort_call_site!("only type parameter are supported"),
        };
        where_clause.extend(quote! { #type_var: #croot::cbor::FromCbor, });
    }

    quote! {
        impl#generics #croot::cbor::FromCbor for #name#generics #where_clause {
            fn from_cbor(value: #croot::cbor::Cbor) -> #croot::Result<#name#generics> {
                use #croot::{cbor::IntoCbor, Error};

                let mut items = Vec::<#croot::cbor::Cbor>::from_cbor(value)?;

                #preamble

                Ok(#name #token_fields)
            }
        }
    }
}

fn impl_cborize_enum(input: &DeriveInput, crate_local: bool) -> TokenStream {
    let name = &input.ident;
    let generics = no_default_generics(input);

    let mut ts = TokenStream::new();
    match &input.data {
        Data::Enum(ast) => {
            let variants: Vec<&Variant> = ast.variants.iter().collect();
            ts.extend(from_enum_to_cbor(name, &generics, &variants, crate_local));
            ts.extend(from_cbor_to_enum(name, &generics, &variants, crate_local));
            ts
        }
        _ => unreachable!(),
    }
}

fn from_enum_to_cbor(
    name: &Ident,
    generics: &Generics,
    variants: &[&Variant],
    crate_local: bool,
) -> TokenStream {
    let id_declr = let_id(name, generics);
    let croot = get_root_crate(crate_local);
    let preamble = quote! {
        let val: #croot::cbor::Cbor = {
            #id_declr;
            #croot::cbor::Tag::from_identifier(id).into()
        };
        items.push(val);
    };

    let mut tok_variants: TokenStream = TokenStream::new();
    for variant in variants.iter() {
        let variant_name = &variant.ident;
        let variant_lit = variant.ident.to_string();
        let arm = match &variant.fields {
            Fields::Unit => {
                quote! { #name::#variant_name => #variant_lit.into_cbor()? }
            }
            Fields::Named(fields) => {
                let (params, body) = named_var_fields_to_cbor(fields, croot.clone());
                quote! {
                    #name::#variant_name{#params} => {
                        items.push(#variant_lit.into_cbor()?);
                        #body
                    },
                }
            }
            Fields::Unnamed(fields) => {
                let (params, body) = unnamed_fields_to_cbor(fields, croot.clone());
                quote! {
                    #name::#variant_name(#params) => {
                        items.push(#variant_lit.into_cbor()?);
                        #body
                    },
                }
            }
        };
        tok_variants.extend(arm)
    }

    let mut where_clause = match &generics.where_clause {
        Some(where_clause) => quote! { #where_clause },
        None => quote! { where },
    };
    for param in generics.params.iter() {
        let type_var = match param {
            GenericParam::Type(param) => &param.ident,
            _ => abort_call_site!("only type parameter are supported"),
        };
        where_clause.extend(quote! { #type_var: #croot::cbor::IntoCbor, });
    }

    quote! {
        impl#generics #croot::cbor::IntoCbor for #name#generics #where_clause {
            fn into_cbor(self) -> #croot::Result<#croot::cbor::Cbor> {
                let value = self;

                let mut items: Vec<#croot::cbor::Cbor> = Vec::default();

                #preamble
                match value {
                    #tok_variants
                }
                Ok(items.into_cbor()?)
            }
        }
    }
}

fn from_cbor_to_enum(
    name: &Ident,
    generics: &Generics,
    variants: &[&Variant],
    crate_local: bool,
) -> TokenStream {
    let name_lit = name.to_string();
    let id_declr = let_id(name, generics);
    let croot = get_root_crate(crate_local);
    let preamble = quote! {
        // validate the cbor msg for this type.
        if items.len() < 2 {
            #croot::err_at!(FailConvert, msg: "empty msg for {}", #name_lit)?;
        }
        let data_id = items.remove(0);
        let type_id: #croot::cbor::Cbor= {
            #id_declr;
            #croot::cbor::Tag::from_identifier(id).into()
        };
        if data_id != type_id {
            #croot::err_at!(FailConvert, msg: "bad {}", #name_lit)?
        }

        let variant_name = String::from_cbor(items.remove(0))?;
    };

    let mut check_variants: TokenStream = TokenStream::new();
    for variant in variants.iter() {
        let variant_lit = &variant.ident.to_string();
        let arm = match &variant.fields {
            Fields::Named(fields) => {
                let n_fields = fields.named.len();
                quote! {
                   #variant_lit => {
                        if #n_fields != items.len() {
                            #croot::err_at!(
                                FailConvert, msg: "bad arity {} {}",
                                #n_fields, items.len()
                            )?;
                        }
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let n_fields = fields.unnamed.len();
                quote! {
                    #variant_lit => {
                        if #n_fields != items.len() {
                            #croot::err_at!(
                                FailConvert, msg: "bad arity {} {}",
                                #n_fields, items.len()
                            )?;
                        }
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    #variant_lit => {
                        if items.len() > 0 {
                            #croot::err_at!(
                                FailConvert, msg: "bad arity {}", items.len()
                            )?;
                        }
                    }
                }
            }
        };
        check_variants.extend(arm)
    }

    let mut tok_variants: TokenStream = TokenStream::new();
    for variant in variants.iter() {
        let variant_name = &variant.ident;
        let variant_lit = &variant.ident.to_string();
        let arm = match &variant.fields {
            Fields::Unit => quote! {
                #variant_lit => #name::#variant_name
            },
            Fields::Named(fields) => {
                let (_, body) = cbor_to_named_var_fields(fields, croot.clone());
                quote! { #variant_lit => #name::#variant_name { #body }, }
            }
            Fields::Unnamed(fields) => {
                let (_, body) = cbor_to_unnamed_fields(fields, croot.clone());
                quote! { #variant_lit => #name::#variant_name(#body), }
            }
        };
        tok_variants.extend(arm);
    }

    let mut where_clause = match &generics.where_clause {
        Some(where_clause) => quote! { #where_clause },
        None => quote! { where },
    };
    for param in generics.params.iter() {
        let type_var = match param {
            GenericParam::Type(param) => &param.ident,
            _ => abort_call_site!("only type parameter are supported"),
        };
        where_clause.extend(quote! { #type_var: #croot::cbor::FromCbor, });
    }
    quote! {
        impl#generics #croot::cbor::FromCbor for #name#generics #where_clause {
            fn from_cbor(value: #croot::cbor::Cbor) -> #croot::Result<#name#generics> {
                use #croot::{cbor::IntoCbor, Error};

                let mut items =  Vec::<#croot::cbor::Cbor>::from_cbor(value)?;

                #preamble

                match variant_name.as_str() {
                    #check_variants
                    _ => #croot::err_at!(
                        FailConvert, msg: "invalid variant_name {}", variant_name
                    )?,
                }

                let val = match variant_name.as_str() {
                    #tok_variants
                    _ => #croot::err_at!(
                        FailConvert, msg: "invalid variant_name {}", variant_name
                    )?,
                };
                Ok(val)
            }
        }
    }
}

fn named_fields_to_cbor(fields: &FieldsNamed, croot: TokenStream) -> TokenStream {
    let mut tokens = TokenStream::new();
    for field in fields.named.iter() {
        let is_bytes = is_bytes_ty(&field.ty);

        match &field.ident {
            Some(field_name) if is_bytes => tokens.extend(quote! {
                items.push(#croot::cbor::Cbor::bytes_into_cbor(value.#field_name)?);
            }),
            Some(field_name) => tokens.extend(quote! {
                items.push(value.#field_name.into_cbor()?);
            }),
            None => (),
        }
    }
    tokens
}

fn named_var_fields_to_cbor(
    fields: &FieldsNamed,
    croot: TokenStream,
) -> (TokenStream, TokenStream) {
    let mut params = TokenStream::new();
    let mut body = TokenStream::new();
    for field in fields.named.iter() {
        let is_bytes = is_bytes_ty(&field.ty);

        let field_name = field.ident.as_ref().unwrap();
        params.extend(quote! { #field_name, });

        match &field.ident {
            Some(field_name) if is_bytes => body.extend(quote! {
                items.push(#croot::cbor::Cbor::bytes_into_cbor(#field_name)?);
            }),
            Some(field_name) => body.extend(quote! {
                items.push(#field_name.into_cbor()?);
            }),
            None => (),
        }
    }
    (params, body)
}

fn unnamed_fields_to_cbor(
    fields: &FieldsUnnamed,
    croot: TokenStream,
) -> (TokenStream, TokenStream) {
    let mut params = TokenStream::new();
    let mut body = TokenStream::new();
    for (field_name, field) in UNNAMED_FIELDS.iter().zip(fields.unnamed.iter()) {
        let field_name = Ident::new(field_name, field.span());
        let is_bytes = is_bytes_ty(&field.ty);

        params.extend(quote! { #field_name, });

        if is_bytes {
            body.extend(quote! {
                items.push(#croot::cbor::Cbor::bytes_into_cbor(#field_name)?);
            });
        } else {
            body.extend(quote! {
                items.push(#field_name.into_cbor()?);
            });
        }
    }
    (params, body)
}

fn cbor_to_named_fields(fields: &FieldsNamed, croot: TokenStream) -> TokenStream {
    let mut tokens = TokenStream::new();
    for field in fields.named.iter() {
        let is_bytes = is_bytes_ty(&field.ty);

        let field_name = field.ident.as_ref().unwrap();
        let ty = &field.ty;
        let field_tokens = if is_bytes {
            quote! {
                #field_name: items.remove(0).into_bytes()?,
            }
        } else {
            quote! {
                #field_name: <#ty as #croot::cbor::FromCbor>::from_cbor(items.remove(0))?,
            }
        };
        tokens.extend(field_tokens);
    }
    tokens
}

fn cbor_to_named_var_fields(
    fields: &FieldsNamed,
    croot: TokenStream,
) -> (TokenStream, TokenStream) {
    let mut params = TokenStream::new();
    let mut body = TokenStream::new();
    for field in fields.named.iter() {
        let is_bytes = is_bytes_ty(&field.ty);

        let field_name = field.ident.as_ref().unwrap();
        params.extend(quote! { #field_name, });

        let ty = &field.ty;
        if is_bytes {
            body.extend(quote! {
                #field_name: items.remove(0).into_bytes()?,
            });
        } else {
            body.extend(quote! {
                #field_name: <#ty as #croot::cbor::FromCbor>::from_cbor(items.remove(0))?,
            });
        }
    }
    (params, body)
}

fn cbor_to_unnamed_fields(
    fields: &FieldsUnnamed,
    croot: TokenStream,
) -> (TokenStream, TokenStream) {
    let mut params = TokenStream::new();
    let mut body = TokenStream::new();
    for (field_name, field) in UNNAMED_FIELDS.iter().zip(fields.unnamed.iter()) {
        let field_name = Ident::new(field_name, field.span());
        let is_bytes = is_bytes_ty(&field.ty);

        params.extend(quote! { #field_name, });

        let ty = &field.ty;
        if is_bytes {
            body.extend(quote! { items.remove(0).into_bytes()?, });
        } else {
            body.extend(
                quote! { <#ty as #croot::cbor::FromCbor>::from_cbor(items.remove(0))?, },
            );
        }
    }
    (params, body)
}

fn let_id(name: &Ident, generics: &Generics) -> TokenStream {
    if generics.params.is_empty() {
        quote! { let id = #name::ID.into_cbor()? }
    } else {
        quote! { let id = #name::#generics::ID.into_cbor()? }
    }
}

fn get_root_crate(crate_local: bool) -> TokenStream {
    if crate_local {
        quote! { crate }
    } else {
        quote! { ::mkit }
    }
}

fn no_default_generics(input: &DeriveInput) -> Generics {
    let mut generics = input.generics.clone();
    generics.params.iter_mut().for_each(|param| match param {
        GenericParam::Type(param) => {
            param.eq_token = None;
            param.default = None;
        }
        _ => (),
    });
    generics
}

fn is_bytes_ty(ty: &syn::Type) -> bool {
    match ty::subty_of_vec(ty) {
        Some(subty) => ty::ty_u8(subty),
        None => false,
    }
}
