use mir::*;
use std::collections::HashMap;
use super::builder::Builder;
use super::local_stack::LocalStack;
use syntax::ast;
use syntax::codemap::Span;

impl<'a, 'b: 'a> Builder<'a, 'b> {
    pub fn block(&mut self, block: BasicBlock, local_stack: &mut LocalStack) -> Vec<ast::Stmt> {
        let block_data = &self.mir[block];

        assert!(block_data.terminator.is_some(),
                "block does not have a terminator");

        let scope_block = self.build_scope_block(block);

        // Create a stack to track renames while expanding this block.
        let (terminated, stmts) = self.scope_block(block, scope_block, local_stack);

        if !terminated {
            span_bug!(
                self.cx,
                block_data.span,
                "scope block did not terminate?");
        }


        stmts
    }

    /// Rebuild an AST block from a MIR block.
    ///
    /// Stateful doesn't need to care about drops because that's automatically handled by the
    /// compiler when we raise MIR back into the AST. Unfortunately, MIR flattens out all of our
    /// scopes, so we need to rebuild this from the visibility scope information we parsed out
    /// during MIR construction.
    fn build_scope_block(&self, block: BasicBlock) -> ScopeBlock<'a> {
        // First, we need to group all our declarations by their scope.
        let mut scope_decls = HashMap::new();

        if let Some(initialized) = self.assignments.initialized(block) {
            for &local in initialized {
                let mut scope = self.mir.local_decls[local].source_info.scope;

                // In AST, non-argument variables are actually defined in their parent scope.
                if scope != ARGUMENT_VISIBILITY_SCOPE {
                    scope = self.mir.visibility_scopes[scope].parent_scope.unwrap();
                }

                if local != COROUTINE_ARGS {
                    scope_decls.entry(scope).or_insert_with(Vec::new).push(local);
                }
            }
        }

        // Next, we'll start rebuilding the scope stack. We'll start with the root block, and work
        // our ways up. Each time we build a block, we pull all the associated declarations and
        // move them into this block.
        let mut stack = vec![
            ScopeBlock::new(
                ARGUMENT_VISIBILITY_SCOPE,
                scope_decls.remove(&ARGUMENT_VISIBILITY_SCOPE),
            ),
        ];

        let block_data = &self.mir[block];

        // Next, loop through the statements and insert them into the right block. If they have a
        // different scope, we'll push and pop the scopes until.
        for stmt in block_data.statements() {
            let last_scope = stack.last().unwrap().scope;
            let current_scope = stmt.source_info.scope;

            // Things are a little tricky when this statement's scope is different from the prior
            // statement. Consider:
            //
            // ```rust
            // {
            //     {
            //         a;
            //     }
            // }
            // {
            //     {
            //         b;
            //     }
            // }
            // ```
            //
            // In MIR, we'll just have two statements, `a;` and `b;` with different scopes. We need
            // to pop and push on scopes in order to get to the right level.
            if current_scope != last_scope {
                let last_scope_path = &self.scope_paths[&last_scope];
                let current_scope_path = &self.scope_paths[&current_scope];

                // Find the last common anscestor between the paths.
                let common = last_scope_path.iter()
                    .zip(current_scope_path.iter())
                    .take_while(|&(lhs, rhs)| lhs == rhs)
                    .count();

                let last_scope_path = last_scope_path.split_at(common).1;
                let current_scope_path = current_scope_path.split_at(common).1;

                // Walk down the scope tree to the last common point. We'll commit all the scopes
                // along the way.
                for scope in last_scope_path {
                    let scope_block = stack.pop().unwrap();
                    assert_eq!(*scope, scope_block.scope);

                    let scope_block = ScopeStatement::Block(scope_block);
                    stack.last_mut().unwrap().push(scope_block);
                }

                // Walk up the scope tree to the current scope point, pushing new blocks along the
                // way.
                for scope in current_scope_path {
                    let scope_block = ScopeBlock::new(
                            *scope,
                            scope_decls.remove(scope));
                    stack.push(scope_block);
                }
            }

            let stmt = ScopeStatement::Statement(stmt);
            stack.last_mut().unwrap().push(stmt);
        }

        // Finally, push the terminator if we have one.
        if let Some(ref terminator) = block_data.terminator {
            let terminator = ScopeStatement::Terminator(terminator);
            stack.last_mut().unwrap().push(terminator);
        }

        // Pop off the remaining scopes.
        while stack.len() != 1 {
            let scope_block = stack.pop().unwrap();
            let scope_block = ScopeStatement::Block(scope_block);
            stack.last_mut().unwrap().push(scope_block);
        }

        let scope_block = stack.pop().unwrap();

        assert!(stack.is_empty(), "stack is not empty: {:#?}", stack);
        assert_eq!(scope_block.scope,
                   ARGUMENT_VISIBILITY_SCOPE,
                   "block is not root scope: {:?}",
                   scope_block);
        assert!(scope_decls.is_empty(), "scope decls is not empty: {:?}", scope_decls);

        scope_block
    }

    fn scope_block(&mut self,
                   block: BasicBlock,
                   scope_block: ScopeBlock,
                   local_stack: &mut LocalStack) -> (bool, Vec<ast::Stmt>) {
        let mut terminated = false;

        let stmts = scope_block.stmts.into_iter()
            .flat_map(|stmt| {
                match stmt {
                    ScopeStatement::Declare(local) => {
                        self.declare(local_stack, local)
                    }
                    ScopeStatement::Statement(stmt) => {
                        self.stmt(block, local_stack, stmt)
                    }
                    ScopeStatement::Block(scope_block) => {
                        let (terminated_, stmts) = local_stack.in_scope(|local_stack| {
                            self.scope_block(block, scope_block, local_stack)
                        });
                        terminated = terminated_;

                        let stmts = vec![
                            self.ast_builder.stmt().semi().block()
                                .with_stmts(stmts)
                                .build()
                        ];

                        stmts
                    }
                    ScopeStatement::Terminator(terminator) => {
                        terminated = true;
                        self.terminator(local_stack, terminator)
                    }
                }
            })
            .collect();

        (terminated, stmts)
    }

    fn declare(&mut self, local_stack: &mut LocalStack, local: Local) -> Vec<ast::Stmt> {
        let local_decl = self.mir.local_decl_data(local);

        let ast_builder = self.ast_builder.span(local_decl.source_info.span);

        let mut stmts = local_stack.push(local).into_iter()
            .collect::<Vec<_>>();

        let stmt_builder = match local_decl.mutability {
            ast::Mutability::Mutable => ast_builder.stmt().let_().mut_id(local_decl.name),
            ast::Mutability::Immutable => ast_builder.stmt().let_().id(local_decl.name),
        };

        stmts.push(stmt_builder.build_option_ty(local_decl.ty.clone())
            .build());

        stmts
    }

    fn terminator(&self,
                  local_stack: &mut LocalStack,
                  terminator: &Terminator) -> Vec<ast::Stmt> {
        let span = terminator.source_info.span;
        let ast_builder = self.ast_builder.span(span);

        match terminator.kind {
            TerminatorKind::Goto { target } => {
                self.goto(span, target, local_stack)
            }
            TerminatorKind::Break { target, after_target: _ } => {
                self.goto(span, target, local_stack)
            }
            TerminatorKind::If { ref cond, targets: (then_block, else_block) } => {
                let cond = cond.to_expr(&self.mir.local_decls);

                let then_block = ast_builder
                    .span(self.block_span(then_block))
                    .block()
                    .with_stmts(self.goto(span, then_block, local_stack))
                    .build();

                let else_block = ast_builder
                    .span(self.block_span(else_block))
                    .block()
                    .with_stmts(self.goto(span, else_block, local_stack))
                    .build();

                vec![
                    ast_builder.stmt().expr().if_()
                        .build(cond)
                        .build_then(then_block)
                        .build_else(else_block),
                ]
            }
            TerminatorKind::Match { ref discr, ref arms } => {
                let discr = discr.to_expr(&self.mir.local_decls);

                let arms = arms.iter()
                    .map(|arm| {
                        let (_, stmts) = local_stack.in_scope(|local_stack| {
                            // Push any locals defined in the match arm onto the stack to make sure
                            // values are properly shadowed.
                            for lvalue in arm.lvalues.iter() {
                                if let Lvalue::Local(local) = *lvalue {
                                    local_stack.push(local);
                                }
                            }

                            (true, self.goto(span, arm.block, local_stack))
                        });

                        let ast_builder = ast_builder.span(self.block_span(arm.block));
                        let block = ast_builder.block()
                            .span(self.mir.span)
                            .with_stmts(stmts)
                            .build();

                        ast_builder.arm()
                            .with_pats(arm.pats.iter().cloned())
                            .with_guard(arm.guard.clone())
                            .body().build_block(block)
                    });

                vec![
                    ast_builder.stmt().expr().match_()
                        .build(discr)
                        .with_arms(arms)
                        .build()
                ]
            }
            TerminatorKind::Return => {
                let next_state = ast_builder.expr().path()
                    .span(self.mir.span)
                    .ids(&["ResumeState", "Illegal"])
                    .build();

                match self.mir.state_machine_kind {
                    StateMachineKind::Generator => {
                        vec![
                            /*
                            // generate `let () = return_;` to make sure it's been assigned the
                            // right type.
                            ast_builder.stmt().let_()
                                    .tuple().build()
                                .expr().id("return_"),
                                */
                            ast_builder.stmt().semi().return_expr().tuple()
                                .expr().none()
                                .expr().build(next_state)
                                .build()
                        ]
                    }
                    StateMachineKind::Async => {
                        let return_expr = ast_builder.expr().id("return_");
                        let ready_expr = ast_builder.expr().call()
                            .path()
                                .global()
                                .ids(&["futures", "Async", "Ready"])
                                .build()
                            .with_arg(return_expr)
                            .build();

                        vec![
                            ast_builder.stmt().semi().return_expr().ok().tuple()
                                .expr().build(ready_expr)
                                .expr().build(next_state)
                                .build()
                        ]
                    }
                }
            }
            TerminatorKind::Suspend {
                destination: (_, target),
                ref arg,
            } => {
                let arg = arg.to_expr(&self.mir.local_decls);
                let next_state = self.resume_state_expr(target, local_stack);

                let ast_builder = ast_builder.span(arg.span);
                let expr = ast_builder.expr().tuple()
                    .expr().build(arg)
                    .expr().build(next_state)
                    .build();

                let expr = match self.mir.state_machine_kind {
                    StateMachineKind::Generator => expr,
                    StateMachineKind::Async => ast_builder.expr().ok().build(expr),
                };

                vec![
                    ast_builder.stmt().semi().return_expr()
                        .build(expr)
                ]
            }
        }
    }

    fn goto(&self, span: Span,
            target: BasicBlock,
            local_stack: &LocalStack) -> Vec<ast::Stmt> {
        let next_state = self.internal_state_expr(target, local_stack);

        let ast_builder = self.ast_builder.span(span);
        let next_expr = ast_builder.expr()
            .assign()
            .id("state")
            .build(next_state);

        vec![
            ast_builder.stmt().semi().build(next_expr),
            ast_builder.stmt().semi().continue_(),
        ]
    }
}

#[derive(Debug)]
struct ScopeBlock<'a> {
    scope: VisibilityScope,
    stmts: Vec<ScopeStatement<'a>>,
}

#[derive(Debug)]
enum ScopeStatement<'a> {
    Declare(Local),
    Statement(&'a Statement),
    Block(ScopeBlock<'a>),
    Terminator(&'a Terminator),
}

impl<'a> ScopeBlock<'a> {
    fn new(scope: VisibilityScope, locals: Option<Vec<Local>>) -> Self {
        ScopeBlock {
            scope: scope,
            stmts: locals.unwrap_or_else(Vec::new).into_iter()
                .map(ScopeStatement::Declare)
                .collect(),
        }
    }

    fn push(&mut self, stmt: ScopeStatement<'a>) {
        self.stmts.push(stmt);
    }
}
