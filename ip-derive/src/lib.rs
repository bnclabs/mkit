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
            pub fn to_cbor_msg_name(&self) -> String {
                ::ip_tools::utils::to_snake_case(#name)
            }
        }

        impl ::std::convert::TryFrom<#name> for ::ip_tools::cbor::Cbor {
            type Error = ::ip_tools::cbor::Error;

            fn try_from(value: #name) -> ::ip_tools::cbor::Cbor {
                let mut props: Vec<(::ip_tools::cbor::Key, ::ip_tools::cbor::Cbor)> = vec![];
                #token_builder;
                Ok(props.try_into()?)
            }
        }
    }
}

fn to_cbor_property(field: &Field) -> TokenStream {
    match &field.ident {
        Some(field_name) => {
            let key = cbor::Key::Text(field_name.to_string().to_lowercase());
            let val: ::ip_tools::cbor::Cbor = value.#field_name.try_into()?;
            props.push((key, val))
        }
        None => TokenStream::new(),
    }
}

fn from_cbor_to_type(name: &Ident, fields: &FieldsNamed) -> TokenStream {
    let mut token_builder = quote! {
        let key = cbor::Key::Text("__msg_name".to_string());
        let val: Cbor = #name.to_lower_case().try_into()?;
        props.push((key, val));
    };
    let mut token_builder = quote! {};
    for field in fields.named.iter() {
        token_builder.extend(to_type_field(field));
    }

    quote! {
        impl ::std::convert::TryFrom<::jsondata::Json> for #name {
            type Error = ::jsondata::Error;

            fn try_from(value: ::jsondata::Json) -> ::std::result::Result<#name, Self::Error> {
                use ::std::convert::TryInto;

                Ok(#name {
                    #token_builder
                })
            }
        }
    }
}

fn to_type_field(field: &Field) -> TokenStream {
    match &field.ident {
        Some(field_name) => {
            let key = field_name.to_string().to_lowercase();
            let is_from_str = get_from_str(&field.attrs);
            match (is_from_str, get_try_into(&field.attrs)) {
                (true, _) => quote! {
                    #field_name: {
                        let v: String = match value.get(&("/".to_string() + #key))?.try_into() {
                            Ok(v) => Ok(v),
                            Err(err) => Err(::jsondata::Error::InvalidType(#key.to_string())),
                        }?;
                        match v.parse() {
                            Ok(v) => Ok(v),
                            Err(err) => Err(::jsondata::Error::InvalidType(#key.to_string())),
                        }?
                    },
                },
                (false, Some(intr_type)) => quote! {
                    #field_name: {
                        let v: #intr_type = match value.get(&("/".to_string() + #key))?.try_into() {
                            Ok(v) => Ok(v),
                            Err(err) => Err(::jsondata::Error::InvalidType(#key.to_string())),
                        }?;
                        match v.try_into() {
                            Ok(v) => Ok(v),
                            Err(err) => Err(::jsondata::Error::InvalidType(#key.to_string())),
                        }?
                    },
                },
                (false, None) => quote! {
                    #field_name: match value.get(&("/".to_string() + #key))?.try_into() {
                        Ok(v) => Ok(v),
                        Err(err) => {
                            let msg = format!("{} err: {}", #key.to_string(), err);
                            Err(::jsondata::Error::InvalidType(msg))
                        }
                    }?,
                },
            }
        }
        None => TokenStream::new(),
    }
}
