use data_structures::indexed_vec::Idx;
use mir::*;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use super::builder::Builder;
use super::state::StateKind;
use syntax::ast;
use syntax::codemap::Span;
use syntax::ptr::P;

impl<'a, 'b: 'a> Builder<'a, 'b> {
    pub fn coroutine_state(&self) -> CoroutineState {
        let blocks = &self.resume_blocks;

        let mut variants = Vec::with_capacity(blocks.len());
        let mut seen_ty_param_ids = HashSet::new();
        let mut ty_param_ids = vec![];
        let mut arms = Vec::with_capacity(blocks.len());
        
        for &block in blocks.iter() {
            let (variant, tp) = self.state_variant(block, StateKind::Coroutine);
            variants.push(variant);

            // It's possible for a declaration to be created but not actually get used in the state
            // variables, so we only create a type parameter for a declaration if it's actually
            // used.
            for ty_param_id in tp {
                if !seen_ty_param_ids.contains(&ty_param_id) {
                    seen_ty_param_ids.insert(ty_param_id);
                    ty_param_ids.push(ty_param_id);
                }
            }

            arms.push(self.coroutine_arm(block));
        }

        let generics = self.ast_builder.generics()
            .with_ty_param_ids(ty_param_ids.iter())
            .build();

        let enum_item = self.ast_builder.item().enum_("CoroutineState")
            .generics().with(generics.clone()).build()
            .id("Illegal")
            .with_variants(variants)
            .build();

        let state_path = self.ast_builder
            .path()
                .segment("CoroutineState")
                .with_tys(
                    ty_param_ids.iter().map(|variable| self.ast_builder.ty().id(variable))
                )
                .build()
            .build();

        let default_item = quote_item!(self.cx,
            impl $generics ::std::default::Default for $state_path {
                fn default() -> Self {
                    CoroutineState::Illegal
                }
            }
        ).expect("state default item");

        let stmts = vec![
            self.ast_builder.stmt().build_item(enum_item),
            self.ast_builder.stmt().build_item(default_item),
        ];

        let expr = quote_expr!(self.cx,
            match coroutine_state {
                $arms
                CoroutineState::Illegal => { panic!("illegal state") }
            }
        );

        CoroutineState {
            stmts: stmts,
            expr: expr,
        }
    }

    pub fn coroutine_state_expr(&self, block: BasicBlock) -> P<ast::Expr> {
        self.state_expr(block, StateKind::Coroutine)
    }

    /// Build up an `ast::Arm` for a coroutine state variant. This arm's role is to lift up the
    /// coroutine arguments into the state machine, which is simply generating a conversion like
    /// this:
    ///
    /// ```rust
    /// CoroutineInternal::State1(scope1, scope2) => {
    ///     InternalState::State1(scope1, scope2, args)
    /// }
    /// ```
    fn coroutine_arm(&self, block: BasicBlock) -> ast::Arm {
        let span = self.block_span(block);
        let ast_builder = self.ast_builder.span(span);

        let ids = self.scope_locals[&block].iter()
            .map(|&(scope, _)| ast_builder.id(format!("scope{}", scope.index())))
            .collect::<Vec<_>>();

        let coroutine_path = self.state_path(block, StateKind::Coroutine);
        let coroutine_pat = ast_builder.pat().enum_().build(coroutine_path)
            .with_ids(&ids)
            .build();

        let internal_path = self.state_path(block, StateKind::Internal);
        let internal_expr = ast_builder.expr().call()
            .build_path(internal_path)
            .with_args(ids.into_iter().map(|id| ast_builder.expr().id(id)))
            .arg().id("args")
            .build();

        ast_builder.arm()
            .with_pat(coroutine_pat)
            .body().build(internal_expr)
    }
}

pub struct CoroutineState {
    pub stmts: Vec<ast::Stmt>,
    pub expr: P<ast::Expr>,
}
