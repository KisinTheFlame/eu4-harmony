//! `#[called_by_game]`: marks a function the game's code calls (a
//! replacement, what a jump stub calls through `call_rust!`, a window
//! procedure or another callback handed to the game or to Windows) and runs
//! its body under the core's `guarded`. A panic there is logged, the player
//! is told and the game ends, instead of the panic crossing into the game's
//! code, where Rust aborts the process without a word (ADR
//! framework-succeeds-or-fails-as-a-whole).
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, ReturnType};

#[proc_macro_attribute]
pub fn called_by_game(args: TokenStream, item: TokenStream) -> TokenStream {
    if !args.is_empty() {
        let args = proc_macro2::TokenStream::from(args);
        return syn::Error::new_spanned(args, "called_by_game takes no arguments").to_compile_error().into();
    }
    let mut function = parse_macro_input!(item as ItemFn);
    if function.sig.abi.is_none() {
        return syn::Error::new_spanned(&function.sig, "the game calls only an extern function").to_compile_error().into();
    }
    let name = function.sig.ident.to_string();
    let output = match &function.sig.output {
        ReturnType::Default => quote!(()),
        ReturnType::Type(_, ty) => quote!(#ty),
    };
    let body = &function.block;
    function.block = syn::parse_quote!({
        crate::guarded(concat!(module_path!(), "::", #name), move || -> #output #body)
    });
    quote!(#function).into()
}
