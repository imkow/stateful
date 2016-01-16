use aster::AstBuilder;
use smir::repr::*;
use syntax::ast;
use syntax::ext::base::ExtCtxt;
use syntax::ptr::P;

pub fn translate(cx: &ExtCtxt, smir: &Smir) -> Option<P<ast::Item>> {
    let ast_builder = AstBuilder::new();

    let item_builder = ast_builder.item().fn_(smir.ident)
        .with_args(smir.fn_decl.inputs.iter().cloned());

    let item_builder = match smir.fn_decl.output {
        ast::FunctionRetTy::NoReturn(..) => item_builder.no_return(),
        ast::FunctionRetTy::DefaultReturn(_) => {
            let ty = quote_ty!(cx, ::std::boxed::Box<::std::iter::Iterator<Item=()>>);
            item_builder.build_return(ty)
        }
        ast::FunctionRetTy::Return(ref ty) => {
            let ty = quote_ty!(cx, ::std::boxed::Box<::std::iter::Iterator<Item=$ty>>);
            item_builder.build_return(ty)
        }
    };

    let builder = Builder {
        cx: cx,
        ast_builder: ast_builder,
        smir: smir,
    };

    let start_state_expr = builder.state_expr(START_BLOCK);
    let (state_enum, state_default, state_arms) =
        builder.state_enum_default_and_arms();

    let block = quote_block!(cx, {
        struct Wrapper<S, F> {
            state: S,
            next: F,
        }

        impl<S, T, F> Wrapper<S, F>
            where F: Fn(S) -> (Option<T>, S),
        {
            fn new(initial_state: S, next: F) -> Self {
                Wrapper {
                    state: initial_state,
                    next: next,
                }
            }
        }

        impl<S, T, F> Iterator for Wrapper<S, F>
            where S: Default,
                  F: Fn(S) -> (Option<T>, S)
        {
            type Item = T;

            fn next(&mut self) -> Option<Self::Item> {
                let old_state = mem::replace(&mut self.state, S::default());
                let (value, next_state) = (self.next)(old_state);
                self.state = next_state;
                value
            }
        }

        $state_enum
        $state_default

        Box::new(Wrapper::new(
            $start_state_expr,
            |mut state| {
                loop {
                    match state {
                        $state_arms
                    }
                }
            }
        ))
    });

    let item = item_builder.build(block);

    Some(item)
}

pub struct Builder<'a> {
    cx: &'a ExtCtxt<'a>,
    ast_builder: AstBuilder,
    smir: &'a Smir,
}

mod block;
mod path;
mod state;
mod stmt;