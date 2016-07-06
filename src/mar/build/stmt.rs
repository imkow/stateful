use aster::AstBuilder;
use mar::build::Builder;
use mar::repr::*;
use syntax::ast::{self, StmtKind};
use syntax::codemap::Span;
use syntax::ptr::P;

impl<'a, 'b: 'a> Builder<'a, 'b> {
    pub fn stmts(&mut self,
                 extent: CodeExtent,
                 mut block: BasicBlock,
                 stmts: &[ast::Stmt]) -> BasicBlock {
        for stmt in stmts {
            // Why don't you terminate each block here?
            block = self.stmt(extent, block, stmt);
        }

        block
    }

    pub fn stmt(&mut self,
                extent: CodeExtent,
                block: BasicBlock,
                stmt: &ast::Stmt) -> BasicBlock {
        match stmt.node {
            StmtKind::Expr(ref expr) | StmtKind::Semi(ref expr) => {
                // Ignore empty statements.
                if expr_is_empty(expr) {
                    // println!("empty expr: {:#?}", expr);
                    block
                } else {
                    self.expr(extent, block, expr)
                }
            }
            StmtKind::Local(ref local) => {
                // println!("local: {:#?}", local);
                self.local(extent, block, stmt.span, local)
                
            }
            StmtKind::Item(..) => {
                self.cx.span_bug(stmt.span, "Cannot handle item declarations yet");
            }
            StmtKind::Mac(ref mac) => {
                println!("macro: {:#?}", stmt);
                let (ref mac, _, _) = **mac;
                match self.mac(block, mac) {
                    Some(block) => block,
                    None => self.into(extent, block, stmt.clone()),
                }
            }
        }
    }

    fn local(&mut self,
             extent: CodeExtent,
             block: BasicBlock,
             span: Span,
             local: &P<ast::Local>) -> BasicBlock {
        if local.init.is_none() {
            self.cx.span_bug(span, &format!("Local variables need initializers at the moment"));
        }

        // println!("self 1: {:#?}", self.cfg);
        let block2 = self.expr(extent, block, &local.init.clone().unwrap());
        for (idx, bb) in self.cfg.basic_blocks.iter().enumerate() {
            println!("idx: {}, stmts: {:?}", idx, bb.statements);
        }
        // println!("self 2: {:#?}", self.cfg);
        println!("blocks: b1: {:?}, b2: {:?}", block, block2);

        let init = if block == block2 {
            local.init.clone()
        } else {
            Some(match self.cfg.basic_blocks[block2.index() as usize - 1].statements[0] {
                Statement::Expr(ref stmt) => {
                    match stmt.node {
                        ast::StmtKind::Semi(ref expr) | ast::StmtKind::Expr(ref expr) => expr,
                        _ => unreachable!(),
                    }
                },
                _ => unreachable!(),
            }.clone())
        };


        let block = block2;

        // println!("init: {:?}", init);
        // println!("pat: {:?}", local.pat.clone());

        let mut i = 0u32;
        for (decl, _) in self.get_decls_from_pat(&local.pat) {
            let lvalue = self.cfg.var_decl_data(decl).ident;
            // println!("self 3 - {}: {:#?}", i, self.cfg);
            
            let alias = self.find_decl(lvalue).map(|alias| {
                // println!("self 4 - {}: {:#?}", i, self.cfg);
                let v = self.alias(block, span, alias);
                // println!("self 5 - {}: {:#?}", i, self.cfg);
                v
            });

            self.schedule_drop(span, extent, decl, alias);
            // println!("self 6 - {}: {:#?}", i, self.cfg);

            i += 1;
        }

        // println!("self 7: {:#?}", self.cfg);

        self.cfg.push(block, Statement::Let {
            span: span,
            pat: local.pat.clone(),
            ty: local.ty.clone(),
            // init: local.init.clone(),
            init: init,
        });
        // println!("self 8: {:#?}", self.cfg);

        block
        // block2
    }

    fn alias(&mut self,
             block: BasicBlock,
             span: Span,
             decl: VarDecl) -> Alias {
        let lvalue = self.cfg.var_decl_data(decl).ident;

        let ast_builder = AstBuilder::new();
        let alias = ast_builder.id(format!("{}_shadowed_{}", lvalue, decl.index()));

        self.cfg.push(block, Statement::Let {
            span: span,
            pat: ast_builder.pat().id(alias),
            ty: None,
            init: Some(ast_builder.expr().id(lvalue)),
        });

        Alias {
            lvalue: alias,
            decl: decl,
        }
    }

    pub fn into_stmt(&mut self,
                     _extent: CodeExtent,
                     block: BasicBlock,
                     stmt: ast::Stmt) -> BasicBlock {
        self.cfg.push(block, Statement::Expr(stmt));
        block
    }
}

fn stmt_is_empty(stmt: &ast::Stmt) -> bool {
    match stmt.node {
        ast::StmtKind::Expr(ref e) | ast::StmtKind::Semi(ref e) => expr_is_empty(e),
        _ => false
    }
}

fn expr_is_empty(expr: &ast::Expr) -> bool {
    match expr.node {
        ast::ExprKind::Block(ref block) => {
            for stmt in block.stmts.iter() {
                if !stmt_is_empty(stmt) {
                    return false;
                }
            }

            true
        }
        _ => {
            false
        }
    }
}
