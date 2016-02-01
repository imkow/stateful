use mar::repr::*;
use mar::trans::Builder;
use syntax::ast;
use syntax::ptr::P;

impl<'a> Builder<'a> {
    pub fn stmt(&self, _block: BasicBlock, stmt: &Statement) -> Vec<P<ast::Stmt>> {
        match *stmt {
            Statement::Expr(ref stmt) => vec![stmt.clone()],
            Statement::Let { span, ref pat, ref ty, ref init } => {
                vec![
                    self.ast_builder.span(span).stmt()
                        .build_let(pat.clone(), ty.clone(), init.clone())
                ]

            }
            Statement::Drop { span, ref lvalue, ref alias } => {
                let mut stmts = vec![
                    self.ast_builder
                        .span(span)
                        .stmt().semi().call()
                            .path()
                                .global()
                                .ids(&["std", "mem", "drop"])
                                .build()
                        .arg().id(lvalue)
                        .build()
                ];

                match *alias {
                    Some(ref alias) => {
                        let (mode, ident) = {
                            let decl = self.mar.var_decl_data(alias.decl);
                            let mode = ast::BindingMode::ByValue(decl.mutability);
                            (mode, decl.ident)
                        };

                        stmts.push(self.ast_builder.span(span).stmt()
                            .let_().build_id(mode, ident, None)
                            .expr().id(alias.lvalue)
                        );
                    }
                    None => { }
                }

                stmts
            }
        }
    }
}