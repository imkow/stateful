use mar::build::Builder;
use mar::repr::*;
use syntax::ast::{self, Stmt, StmtKind};
use syntax::ptr::P;

pub trait EvalInto {
    fn eval_into(self,
                 builder: &mut Builder,
                 extent: CodeExtent,
                 block: BasicBlock) -> BasicBlock;
}

impl<'a, 'b: 'a> Builder<'a, 'b> {
    pub fn into<E>(&mut self,
                   extent: CodeExtent,
                   block: BasicBlock,
                   expr: E) -> BasicBlock
        where E: EvalInto + ::std::fmt::Debug
    {
        // println!("into() called in {:#?}", expr);
        expr.eval_into(self, extent, block)
    }
}

impl<'a> EvalInto for &'a P<ast::Block> {
    fn eval_into(self,
                 builder: &mut Builder,
                 extent: CodeExtent,
                 block: BasicBlock) -> BasicBlock {
        builder.ast_block(extent, block, self)
    }
}

impl EvalInto for ast::Stmt {
    fn eval_into(self,
                 builder: &mut Builder,
                 extent: CodeExtent,
                 block: BasicBlock) -> BasicBlock {
        builder.into_stmt(extent, block, self);
        block
    }
}

impl<'a> EvalInto for &'a P<ast::Expr> {
    fn eval_into(self,
                 builder: &mut Builder,
                 extent: CodeExtent,
                 block: BasicBlock) -> BasicBlock {
        self.clone().eval_into(builder, extent, block)
    }
}

impl EvalInto for P<ast::Expr> {
    fn eval_into(self,
                 builder: &mut Builder,
                 extent: CodeExtent,
                 block: BasicBlock) -> BasicBlock {
        let span = self.span.clone();

        let stmt = Stmt {
            id: ast::DUMMY_NODE_ID,
            node: StmtKind::Semi(self),
            span: span,
        };
        builder.into(extent, block, stmt)
    }
}

impl<'a> EvalInto for &'a Option<P<ast::Expr>> {
    fn eval_into(self,
                 builder: &mut Builder,
                 extent: CodeExtent,
                 block: BasicBlock) -> BasicBlock {
        if let Some(ref expr) = *self {
            builder.expr(extent, block, expr)
        } else {
            block
        }
    }
}

impl EvalInto for Option<P<ast::Expr>> {
    fn eval_into(self,
                 builder: &mut Builder,
                 extent: CodeExtent,
                 block: BasicBlock) -> BasicBlock {
        if let Some(ref expr) = self {
            builder.expr(extent, block, expr)
        } else {
            block
        }
    }
}
