extern crate proc_macro;

use proc_macro2::TokenStream;
use proc_macro_error::{abort_call_site, proc_macro_error};
use quote::quote;
use syn::*;

#[proc_macro_derive(Cborize, attributes(cbor))]
#[proc_macro_error]
pub fn cborize_type(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input: DeriveInput = syn::parse(input).unwrap();
    let gen = impl_cborize_type(&input);
    gen.into()
}

fn impl_cborize_type(input: &DeriveInput) -> TokenStream {
    let name = &input.ident;
    match &input.data {
        Data::Struct(ast) => match &ast.fields {
            Fields::Named(fields) => {
                let mut ts = from_type_to_cbor(name, fields);
                ts.extend(from_cbor_to_type(name, fields));
                ts
            }
            _ => abort_call_site!("cbor only supports named fields"),
        },
        _ => abort_call_site!("cbor only supports named structs"),
    }
}

fn from_type_to_cbor(name: &Ident, fields: &FieldsNamed) -> TokenStream {
    let mut token_builder = quote! {
        let key = cbor::Key::Text("__msg_name".to_string());
        let val: Cbor = #name.to_lower_case().try_into()?;
        props.push((key, val));
    };
    for field in fields.named.iter() {
        token_builder.extend(to_cbor_property(field));
    }

    quote! {
        impl #name {
            pub fn to_cbor_msg_name() -> String {
                ::mkit::utils::to_snake_case(#name)
            }
        }

        impl ::std::convert::TryFrom<#name> for ::mkit::cbor::Cbor {
            type Error = ::mkit::cbor::Error;

            fn try_from(value: #name) -> ::mkit::cbor::Cbor {
                let mut props: Vec<(::mkit::cbor::Key, ::mkit::cbor::Cbor)> = vec![];
                #token_builder;
                Ok(props.try_into()?)
            }
        }
    }
}

fn to_cbor_property(field: &Field) -> TokenStream {
    match &field.ident {
        Some(field_name) => quote! {
            let key = cbor::Key::Text(field_name.to_string().to_lowercase());
            let val: ::mkit::cbor::Cbor = value.#field_name.try_into()?;
            props.push((key, val))
        },
        None => TokenStream::new(),
    }
}

fn from_cbor_to_type(name: &Ident, fields: &FieldsNamed) -> TokenStream {
    let mut token_builder = quote! {
        let mut props: Vec<(::mkit::cbor::Key, ::mkit::cbor::Cbor)> = value.try_into()?;
        match props.remove(0) {
            (::mkit::cbor::Key::Text(key), name) => {
                let name: String = name.try_into()?;
                if key != "__msg_name" || name != #name.to_cbor_msg_name() {
                    err_at!(FailConvert, msg: "wrong msg {} {}", key, #name.to_cbor_msg_name())?;
                }
            }
            _ => {
                err_at!(FailConvert, msg: "not a msg {}", #name.to_cbor_msg_name())?;
            }
        }

        let mut props: Vec<(::mkit::cbor::Key, ::mkit::cbor::Cbor)> = props.map(|(key, val)| {
            let key: String = key.try_into()?;
            (key, val)
        }).collect();

        props.sort_by(|(key1, _), (key2, _)| key1.cmp(key2));
    };

    for field in fields.named.iter() {
        token_builder.extend(to_type_field(field));
    }

    quote! {
        impl ::std::convert::TryFrom<::mkit::cbor::Cbor> for #name {
            type Error = ::mkit::Error;

            fn try_from(value: ::mkit::cbor::Cbor) -> ::std::result::Result<#name, Self::Error> {
                use ::std::convert::TryInto;

                Ok(#name {
                    #token_builder
                })
            }
        }
    }
}

fn to_type_field(field: &Field) -> TokenStream {
    let field_name = match &field.ident {
        Some(field_name) => field_name,
        None => return TokenStream::new(),
    };

    quote! {
        #field_name: match props.binary_search_by(|(key, _)| key.cmp(#field_name)) {
            Ok(index) => {
                let (_, value) = props.remove(index);
                value.try_into()?,
            }
            err => err_at!(FailConvert, "field not found {}", #field_name),
        },
    }
}
