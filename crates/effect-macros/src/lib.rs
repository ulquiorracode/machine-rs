//! Procedural macros for `effect-rs`.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Pat, parse_macro_input};

/// Syntax for handling effectful computations:
///
/// ```rust,ignore
/// effect_match!(machine, {
///     perform AppEffect::Ask(ask) => {
///         AppResume::Str("Arthur")
///     }
///     return val => {
///         val
///     }
/// });
/// ```
#[proc_macro]
pub fn effect_match(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as EffectMatchInput);
    let machine_expr = &parsed.machine;
    let effect_arms = &parsed.effect_arms;
    let return_pat = &parsed.return_pat;
    let return_body = &parsed.return_body;

    let expanded = quote! {
        {
            let mut __machine = #machine_expr;
            let mut __resume_val = None;
            loop {
                match ::effect_core::EffectMachine::resume(&mut __machine, __resume_val.take()) {
                    ::effect_core::Step::Yielded(__effect) => {
                        let __response = match __effect {
                            #( #effect_arms )*
                            #[allow(unreachable_patterns)]
                            _ => panic!("Unhandled algebraic effect yielded by computation"),
                        };
                        __resume_val = Some(__response);
                    }
                    ::effect_core::Step::Done(#return_pat) => {
                        break #return_body;
                    }
                }
            }
        }
    };

    TokenStream::from(expanded)
}

struct EffectMatchInput {
    machine: Expr,
    effect_arms: Vec<proc_macro2::TokenStream>,
    return_pat: Pat,
    return_body: Expr,
}

impl Parse for EffectMatchInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let machine: Expr = input.parse()?;
        let _comma: syn::Token![,] = input.parse()?;

        let content;
        syn::braced!(content in input);

        let mut effect_arms = Vec::new();
        let mut return_arm = None;

        while !content.is_empty() {
            if content.peek(syn::Token![return]) {
                let _return_kw: syn::Token![return] = content.parse()?;
                let pat: Pat = Pat::parse_single(&content)?;
                let _fat_arrow: syn::Token![=>] = content.parse()?;
                let body: Expr = content.parse()?;
                let _ = content.parse::<syn::Token![,]>();

                return_arm = Some((pat, body));
            } else if content.peek(syn::Ident) {
                let ident: syn::Ident = content.parse()?;
                if ident == "perform" {
                    let pat: Pat = Pat::parse_single(&content)?;
                    let _fat_arrow: syn::Token![=>] = content.parse()?;
                    let body: Expr = content.parse()?;
                    let _ = content.parse::<syn::Token![,]>();

                    effect_arms.push(quote! {
                        #pat => #body,
                    });
                } else {
                    return Err(syn::Error::new(
                        ident.span(),
                        "Expected `perform` or `return` keyword in effect_match! arm",
                    ));
                }
            } else {
                return Err(
                    content.error("Expected `perform` or `return` keyword in effect_match! arm")
                );
            }
        }

        let (return_pat, return_body) = return_arm.ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "effect_match! requires a `return <pattern> => { ... }` arm",
            )
        })?;

        Ok(EffectMatchInput {
            machine,
            effect_arms,
            return_pat,
            return_body,
        })
    }
}
