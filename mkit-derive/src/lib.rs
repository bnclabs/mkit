extern crate proc_macro;

use lazy_static::lazy_static;
use proc_macro2::TokenStream;
use proc_macro_error::{abort_call_site, proc_macro_error};
use quote::quote;
use syn::*;

lazy_static! {
    pub(crate) static ref UNNAMED_FIELDS: Vec<&'static str> =
        vec!["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l",];
}

#[proc_macro_derive(Cborize, attributes(cbor))]
#[proc_macro_error]
pub fn cborize_type(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input: DeriveInput = syn::parse(input).unwrap();
    let gen = match &input.data {
        Data::Struct(_) => impl_cborize_struct(&input),
        Data::Enum(_) => impl_cborize_enum(&input),
        Data::Union(_) => abort_call_site!("cannot derive Cborize for union"),
    };
    gen.into()
}

fn impl_cborize_struct(input: &DeriveInput) -> TokenStream {
    let name = &input.ident;
    let generics = &input.generics;

    let mut ts = TokenStream::new();
    match &input.data {
        Data::Struct(ast) => {
            ts.extend(from_struct_to_cbor(name, generics, &ast.fields));
            ts.extend(from_cbor_to_struct(name, generics, &ast.fields));
            ts
        }
        _ => unreachable!(),
    }
}

fn from_struct_to_cbor(name: &Ident, generics: &Generics, fields: &Fields) -> TokenStream {
    let preamble = quote! {
        let val = ::mkit::cbor::Cbor::try_from(#name::#generics::to_cbor_msg_name())?;
        items.push(val);
    };

    let token_fields = match fields {
        Fields::Named(fields) => named_fields_to_cbor(fields),
        Fields::Unnamed(_) => abort_call_site!("unnamed struct not supported for Cborize {}", name),
        Fields::Unit => abort_call_site!("unit struct not supported for Cborize {}", name),
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
        where_clause.extend(quote! {
            #type_var: ::std::convert::TryInto<Cbor, Error = ::mkit::Error>,
        });
    }

    quote! {
        impl#generics ::std::convert::TryFrom<#name#generics> for ::mkit::cbor::Cbor #where_clause {
            type Error = ::mkit::Error;

            fn try_from(value: #name#generics) -> ::std::result::Result<::mkit::cbor::Cbor, Self::Error> {
                use ::std::convert::TryInto;

                let mut items: Vec<::mkit::cbor::Cbor> = Vec::default();

                #preamble
                #token_fields;

                Ok(items.try_into()?)
            }
        }
    }
}

fn from_cbor_to_struct(name: &Ident, generics: &Generics, fields: &Fields) -> TokenStream {
    let name_lit = name.to_string();
    let n_fields = match fields {
        Fields::Named(fields) => fields.named.len(),
        Fields::Unnamed(_) => abort_call_site!("unnamed struct not supported for Cborize {}", name),
        Fields::Unit => abort_call_site!("unit struct not supported for Cborize {}", name),
    };

    let preamble = quote! {
        // validate the cbor msg for this type.
        if items.len() == 0 {
            err_at!(FailConvert, msg: "empty msg for {}", #name_lit)?;
        }
        let msg_name = String::try_from(items.remove(0))?;
        let typ_name = #name::#generics::to_cbor_msg_name();
        if msg_name != typ_name {
            err_at!(FailConvert, msg: "bad msg name {} {}", msg_name, typ_name)?;
        }
        if #n_fields != items.len() {
            err_at!(FailConvert, msg: "bad arity {} {}", #n_fields, items.len())?;
        }
    };

    let token_fields = match fields {
        Fields::Named(fields) => cbor_to_named_fields(fields),
        Fields::Unnamed(_) => abort_call_site!("unnamed struct not supported for Cborize {}", name),
        Fields::Unit => abort_call_site!("unit struct not supported for Cborize {}", name),
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
        where_clause.extend(quote! {
            #type_var: ::std::convert::TryFrom<Cbor, Error = ::mkit::Error>,
        });
    }

    quote! {
        impl#generics ::std::convert::TryFrom<::mkit::cbor::Cbor> for #name#generics #where_clause {
            type Error = ::mkit::Error;

            fn try_from(value: ::mkit::cbor::Cbor) -> ::std::result::Result<#name#generics, Self::Error> {
                use ::mkit::Error;
                use ::std::convert::TryInto;

                let mut items: Vec<::mkit::cbor::Cbor> = value.try_into()?;

                #preamble

                Ok(#name {
                    #token_fields
                })
            }
        }
    }
}

fn impl_cborize_enum(input: &DeriveInput) -> TokenStream {
    let name = &input.ident;
    let generics = &input.generics;

    let mut ts = TokenStream::new();
    match &input.data {
        Data::Enum(ast) => {
            let variants: Vec<&Variant> = ast.variants.iter().collect();
            ts.extend(from_enum_to_cbor(name, generics, &variants));
            ts.extend(from_cbor_to_enum(name, generics, &variants));
            ts
        }
        _ => unreachable!(),
    }
}

fn from_enum_to_cbor(name: &Ident, generics: &Generics, variants: &[&Variant]) -> TokenStream {
    let preamble = quote! {
        let val = ::mkit::cbor::Cbor::try_from(#name::#generics::to_cbor_msg_name())?;
        items.push(val);
    };

    let mut tok_variants: TokenStream = TokenStream::new();
    for variant in variants.iter() {
        let variant_name = &variant.ident;
        let arm = match &variant.fields {
            Fields::Unit => {
                quote! { #name::#variant_name => ::mkit::cbor::Cbor::try_from(#variant_name)? }
            }
            Fields::Named(fields) => {
                let (params, body) = named_var_fields_to_cbor(fields);
                quote! {
                    #name::#variant_name(#params) => {
                        ::mkit::cbor::Cbor::try_from(#variant_name)?;
                        #body
                    },
                }
            }
            Fields::Unnamed(fields) => {
                let (params, body) = unnamed_fields_to_cbor(fields);
                quote! {
                    #name::#variant_name(#params) => {
                        ::mkit::cbor::Cbor::try_from(#variant_name)?;
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
        where_clause.extend(quote! {
            #type_var: ::std::convert::TryInto<Cbor, Error = ::mkit::Error>,
        });
    }

    quote! {
        impl#generics ::std::convert::TryFrom<#name#generics> for ::mkit::cbor::Cbor #where_clause {
            type Error = ::mkit::Error;

            fn try_from(value: #name#generics) -> ::std::result::Result<::mkit::cbor::Cbor, Self::Error> {
                use ::std::convert::TryInto;

                let mut items: Vec<::mkit::cbor::Cbor> = Vec::default();

                #preamble
                match value {
                    #tok_variants
                }
                Ok(items.try_into()?)
            }
        }
    }
}

fn from_cbor_to_enum(name: &Ident, generics: &Generics, variants: &[&Variant]) -> TokenStream {
    let name_lit = name.to_string();
    let preamble = quote! {
        // validate the cbor msg for this type.
        if items.len() < 2 {
            err_at!(FailConvert, msg: "empty msg for {}", #name_lit)?;
        }
        let msg_name = String::try_from(items.remove(0))?;
        let typ_name = #name::#generics::to_cbor_msg_name();
        if msg_name != typ_name {
            err_at!(FailConvert, msg: "bad msg name {} {}", msg_name, typ_name)?;
        }

        let variant_name = String::try_from(items.remove(0))?;
    };

    let mut check_variants: TokenStream = TokenStream::new();
    for variant in variants.iter() {
        let variant_name = &variant.ident;
        let arm = match &variant.fields {
            Fields::Named(fields) => {
                let n_fields = fields.named.len();
                quote! {
                    #variant_name => {
                        if #n_fields != items.len() {
                            err_at!(FailConvert, msg: "bad arity {} {}", #n_fields, items.len())?;
                        }
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let n_fields = fields.unnamed.len();
                quote! {
                    #variant_name => {
                        if #n_fields != items.len() {
                            err_at!(FailConvert, msg: "bad arity {} {}", #n_fields, items.len())?;
                        }
                    }
                }
            }
            Fields::Unit => abort_call_site!("unit struct not supported for Cborize {}", name),
        };
        check_variants.extend(arm)
    }

    let mut tok_variants: TokenStream = TokenStream::new();
    for variant in variants.iter() {
        let variant_name = &variant.ident;
        let arm = match &variant.fields {
            Fields::Unit => quote! { #variant_name => #name::#variant_name },
            Fields::Named(fields) => {
                let body = cbor_to_named_var_fields(fields);
                quote! { #variant_name => #name::#variant_name { #body }, }
            }
            Fields::Unnamed(fields) => {
                let body = cbor_to_unnamed_fields(fields);
                quote! { #variant_name => #name::#variant_name(#body) }
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
        where_clause.extend(quote! {
            #type_var: ::std::convert::TryInto<Cbor, Error = ::mkit::Error>,
        });
    }
    quote! {
        impl#generics ::std::convert::TryFrom<::mkit::cbor::Cbor> for #name#generics #where_clause {
            type Error = ::mkit::Error;

            fn try_from(value: ::mkit::cbor::Cbor) -> ::std::result::Result<#name#generics, Self::Error> {
                use ::mkit::Error;
                use ::std::convert::TryInto;

                let mut items: Vec<::mkit::cbor::Cbor> = value.try_into()?;

                #preamble
                #check_variants

                match variant_name {
                    #tok_variants
                }
            }
        }
    }
}

fn named_fields_to_cbor(fields: &FieldsNamed) -> TokenStream {
    let mut tokens = TokenStream::new();
    for field in fields.named.iter() {
        match &field.ident {
            Some(field_name) => tokens.extend(quote! {
                items.push(value.#field_name.try_into()?);
            }),
            None => (),
        }
    }
    tokens
}

fn named_var_fields_to_cbor(fields: &FieldsNamed) -> (TokenStream, TokenStream) {
    let mut params = TokenStream::new();
    let mut body = TokenStream::new();
    for field in fields.named.iter() {
        let field_name = field.ident.as_ref().unwrap();
        params.extend(quote! { #field_name, });
        match &field.ident {
            Some(field_name) => body.extend(quote! {
                items.push(#field_name.try_into()?);
            }),
            None => (),
        }
    }
    (params, body)
}

fn unnamed_fields_to_cbor(fields: &FieldsUnnamed) -> (TokenStream, TokenStream) {
    let mut params = TokenStream::new();
    let mut body = TokenStream::new();
    for (field_name, _field) in UNNAMED_FIELDS.iter().zip(fields.unnamed.iter()) {
        params.extend(quote! { #field_name, });
        body.extend(quote! {
            items.push(::mkit::cbor::Cbor::try_from(#field_name));
        });
    }
    (params, body)
}

fn cbor_to_named_fields(fields: &FieldsNamed) -> TokenStream {
    let mut tokens = TokenStream::new();
    for field in fields.named.iter() {
        let field_name = field.ident.as_ref().unwrap();
        let ty = &field.ty;
        tokens.extend(quote! {
            #field_name: #ty::try_from(items.remove(0))?,
        });
    }
    tokens
}

fn cbor_to_named_var_fields(fields: &FieldsNamed) -> TokenStream {
    let mut tokens = TokenStream::new();
    for field in fields.named.iter() {
        let field_name = field.ident.as_ref().unwrap();
        let ty = &field.ty;
        tokens.extend(quote! {
            #field_name: #ty::try_from(items.remove(0))?,
        });
    }
    tokens
}

fn cbor_to_unnamed_fields(fields: &FieldsUnnamed) -> TokenStream {
    let mut tokens = TokenStream::new();
    for field in fields.unnamed.iter() {
        let ty = &field.ty;
        tokens.extend(quote! { #ty::try_from(items.remove(0))?, });
    }
    tokens
}
