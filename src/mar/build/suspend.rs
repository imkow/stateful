use mar::build::{BlockAnd, BlockAndExtension, Builder};
use mar::repr::*;
use syntax::ast;
use syntax::ptr::P;

impl<'a, 'b: 'a> Builder<'a, 'b> {
    /// Generate code to suspend the coroutine.
    pub fn expr_suspend(&mut self,
                        destination: Lvalue,
                        block: BasicBlock,
                        expr: P<ast::Expr>) -> BlockAnd<()> {
        let expr_span = expr.span;
        let next_block = self.start_new_block(expr.span, Some("AfterSuspend"));

        self.terminate(expr.span, block, TerminatorKind::Suspend {
            rvalue: expr,
            target: next_block,
        });

        // We don't yet support receiving values into the coroutine yet, so just store a `()` in
        // the destination.
        self.push_assign_unit(expr_span, next_block, destination);

        next_block.unit()
    }
}
