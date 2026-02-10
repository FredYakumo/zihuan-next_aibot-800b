use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use std::collections::HashSet;
use syn::{
    braced,
    ext::IdentExt,
    parse::{Parse, ParseStream},
    parse_macro_input,
    spanned::Spanned,
    punctuated::Punctuated,
    token, Expr, Ident, LitBool, LitStr, Result, Token,
};

#[proc_macro]
pub fn node_input(input: TokenStream) -> TokenStream {
    expand_node_ports(input, PortKind::Input)
}

#[proc_macro]
pub fn node_output(input: TokenStream) -> TokenStream {
    expand_node_ports(input, PortKind::Output)
}

enum PortKind {
    Input,
    Output,
}

fn expand_node_ports(input: TokenStream, kind: PortKind) -> TokenStream {
    let ports = parse_macro_input!(input as PortList);

    let mut seen_names: HashSet<String> = HashSet::new();
    for port in &ports.ports {
        if !seen_names.insert(port.name.value()) {
            return syn::Error::new(
                port.name.span(),
                format!("Duplicate port name '{}'", port.name.value()),
            )
            .to_compile_error()
            .into();
        }
    }

    let mut port_tokens: Vec<proc_macro2::TokenStream> = Vec::new();
    for port in ports.ports {
        match port.to_port_tokens() {
            Ok(tokens) => port_tokens.push(tokens),
            Err(err) => return err.to_compile_error().into(),
        }
    }

    let fn_name = match kind {
        PortKind::Input => quote! { input_ports },
        PortKind::Output => quote! { output_ports },
    };

    let expanded = quote! {
        fn #fn_name(&self) -> ::std::vec::Vec<Port> {
            ::std::vec![
                #(#port_tokens),*
            ]
        }
    };

    expanded.into()
}

struct PortList {
    ports: Vec<PortSpec>,
}

impl Parse for PortList {
    fn parse(input: ParseStream) -> Result<Self> {
        let ports = Punctuated::<PortSpec, Token![,]>::parse_terminated(input)?
            .into_iter()
            .collect();
        Ok(Self { ports })
    }
}

struct PortSpec {
    name: LitStr,
    data_type: Expr,
    description: Option<LitStr>,
    optional: bool,
}

impl PortSpec {
    fn to_port_tokens(self) -> Result<proc_macro2::TokenStream> {
        let name = self.name;
        let data_type = datatype_tokens(self.data_type)?;

        let mut tokens = quote! { Port::new(#name, #data_type) };
        if let Some(desc) = self.description {
            tokens = quote! { #tokens.with_description(#desc) };
        }
        if self.optional {
            tokens = quote! { #tokens.optional() };
        }
        Ok(tokens)
    }
}

impl Parse for PortSpec {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Ident) && input.peek2(Token![!]) {
            let name: Ident = input.parse()?;
            if name != "port" {
                return Err(syn::Error::new(name.span(), "Expected 'port!'"));
            }
            input.parse::<Token![!]>()?;
            let content;
            braced!(content in input);
            return parse_port_body(&content);
        }

        if input.peek(token::Brace) {
            let content;
            braced!(content in input);
            return parse_port_body(&content);
        }

        Err(input.error("Expected port!{...} or {...}"))
    }
}

fn parse_port_body(input: ParseStream) -> Result<PortSpec> {
    let items = Punctuated::<PortAttr, Token![,]>::parse_terminated(input)?;

    let mut name: Option<LitStr> = None;
    let mut data_type: Option<Expr> = None;
    let mut description: Option<LitStr> = None;
    let mut optional: Option<bool> = None;

    for item in items {
        match item {
            PortAttr::Name(value) => name = Some(value),
            PortAttr::Type(value) => data_type = Some(value),
            PortAttr::Desc(value) => description = Some(value),
            PortAttr::Optional(value) => optional = Some(value),
            PortAttr::Required(value) => optional = Some(!value),
        }
    }

    let name = name.ok_or_else(|| input.error("Missing required field: name"))?;
    let data_type = data_type.ok_or_else(|| input.error("Missing required field: type"))?;

    Ok(PortSpec {
        name,
        data_type,
        description,
        optional: optional.unwrap_or(false),
    })
}

enum PortAttr {
    Name(LitStr),
    Type(Expr),
    Desc(LitStr),
    Optional(bool),
    Required(bool),
}

impl Parse for PortAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let ident: Ident = input.call(Ident::parse_any)?;

        let key = ident.to_string();
        if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            return match key.as_str() {
                "name" => Ok(PortAttr::Name(input.parse()?)),
                "type" => Ok(PortAttr::Type(input.parse()?)),
                "ty" => Ok(PortAttr::Type(input.parse()?)),
                "desc" => Ok(PortAttr::Desc(input.parse()?)),
                "optional" => Ok(PortAttr::Optional(parse_bool(input)?)),
                "required" => Ok(PortAttr::Required(parse_bool(input)?)),
                _ => Err(syn::Error::new(ident.span(), "Unknown port attribute")),
            };
        }

        match key.as_str() {
            "optional" => Ok(PortAttr::Optional(true)),
            _ => Err(syn::Error::new(ident.span(), "Unexpected flag")),
        }
    }
}

fn parse_bool(input: ParseStream) -> Result<bool> {
    if input.peek(LitBool) {
        let value: LitBool = input.parse()?;
        return Ok(value.value);
    }
    let expr: Expr = input.parse()?;
    match expr {
        Expr::Lit(lit) => {
            if let syn::Lit::Bool(value) = lit.lit {
                Ok(value.value)
            } else {
                Err(syn::Error::new(lit.span(), "Expected boolean literal"))
            }
        }
        _ => Err(syn::Error::new(expr.span(), "Expected boolean literal")),
    }
}

fn datatype_tokens(expr: Expr) -> Result<proc_macro2::TokenStream> {
    match expr {
        Expr::Path(path) => {
            let last = path.path.segments.last().ok_or_else(|| {
                syn::Error::new(path.span(), "Invalid type path")
            })?;

            let ident = &last.ident;
            if path.path.segments.len() == 1 {
                return Ok(quote! { DataType::#ident });
            }

            if path.path.segments.iter().any(|seg| seg.ident == "DataType") {
                return Ok(path.to_token_stream());
            }

            Ok(quote! { DataType::#ident })
        }
        Expr::Call(call) => {
            let func_path = if let Expr::Path(path) = &*call.func {
                path
            } else {
                return Err(syn::Error::new(call.func.span(), "Unsupported type call"));
            };

            let func_name = func_path
                .path
                .segments
                .last()
                .map(|seg| seg.ident.to_string())
                .unwrap_or_default();

            if func_name == "List" {
                if call.args.len() != 1 {
                    return Err(syn::Error::new(call.span(), "List() expects one argument"));
                }
                let inner = call.args.first().cloned().unwrap();
                let inner_tokens = datatype_tokens(inner)?;
                return Ok(quote! { DataType::List(Box::new(#inner_tokens)) });
            }

            if func_name == "Custom" {
                if call.args.len() != 1 {
                    return Err(syn::Error::new(call.span(), "Custom() expects one argument"));
                }
                let inner = call.args.first().cloned().unwrap();
                if let Expr::Lit(lit) = inner {
                    if let syn::Lit::Str(lit_str) = lit.lit {
                        return Ok(quote! { DataType::Custom(#lit_str.to_string()) });
                    }
                }
                return Err(syn::Error::new(call.span(), "Custom() expects a string literal"));
            }

            Err(syn::Error::new(call.span(), "Unsupported type constructor"))
        }
        _ => Err(syn::Error::new(expr.span(), "Unsupported type expression")),
    }
}
