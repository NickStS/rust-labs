use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Expr, Result, Token};

struct Pair {
    key: Expr,
    val: Expr,
}

impl Parse for Pair {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let key: Expr = input.parse()?;
        input.parse::<Token![=>]>()?;
        let val: Expr = input.parse()?;
        Ok(Self { key, val })
    }
}

struct Pairs {
    pairs: Vec<Pair>,
}

impl Parse for Pairs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut pairs = Vec::new();
        while !input.is_empty() {
            let pair: Pair = input.parse()?;
            pairs.push(pair);
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
        }
        Ok(Self { pairs })
    }
}

#[proc_macro]
pub fn btreemap(input: TokenStream) -> TokenStream {
    if input.is_empty() {
        return quote!(::std::collections::BTreeMap::new()).into();
    }

    let parsed = parse_macro_input!(input as Pairs);
    let inserts = parsed.pairs.into_iter().map(|p| {
        let k = p.key;
        let v = p.val;
        quote!(m.insert(#k, #v);)
    });

    quote! {{
        let mut m = ::std::collections::BTreeMap::new();
        #(#inserts)*
        m
    }}
    .into()
}
