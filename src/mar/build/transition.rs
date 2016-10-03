use aster::AstBuilder;
use mar::build::Builder;
use syntax::ast::{self, ExprKind, StmtKind};
use syntax::ext::base::ExtCtxt;
use syntax::ext::tt::transcribe::new_tt_reader;
use syntax::parse::parser::Parser;
use syntax::parse::token::Token;
use syntax::ptr::P;
use syntax::visit;

impl<'a, 'b: 'a> Builder<'a, 'b> {
    pub fn contains_transition<E: ContainsTransition>(&self, expr: E) -> bool {
        expr.contains_transition(self.is_inside_loop())
    }
}

pub trait ContainsTransition {
    fn contains_transition(&self, inside_loop: bool) -> bool;
}

impl<'a> ContainsTransition for &'a P<ast::Block> {
    fn contains_transition(&self, inside_loop: bool) -> bool {
        (**self).contains_transition(inside_loop)
    }
}

impl ContainsTransition for ast::Block {
    fn contains_transition(&self, inside_loop: bool) -> bool {
        let mut visitor = ContainsTransitionVisitor::new(inside_loop);

        visit::Visitor::visit_block(&mut visitor, self);
        visitor.contains_transition
    }
}

impl ContainsTransition for ast::Stmt {
    fn contains_transition(&self, inside_loop: bool) -> bool {
        let mut visitor = ContainsTransitionVisitor::new(inside_loop);

        visit::Visitor::visit_stmt(&mut visitor, self);
        visitor.contains_transition
    }
}

impl<'a> ContainsTransition for &'a P<ast::Expr> {
    fn contains_transition(&self, inside_loop: bool) -> bool {
        (**self).contains_transition(inside_loop)
    }
}

impl ContainsTransition for ast::Expr {
    fn contains_transition(&self, inside_loop: bool) -> bool {
        let mut visitor = ContainsTransitionVisitor::new(inside_loop);

        visit::Visitor::visit_expr(&mut visitor, self);
        visitor.contains_transition
    }
}

impl ContainsTransition for ast::Mac {
    fn contains_transition(&self, _inside_loop: bool) -> bool {
        is_transition_path(&self.node.path)
    }
}

struct ContainsTransitionVisitor {
    inside_loop: bool,
    contains_transition: bool,
}

impl ContainsTransitionVisitor {
    fn new(inside_loop: bool) -> Self {
        ContainsTransitionVisitor {
            inside_loop: inside_loop,
            contains_transition: false,
        }
    }
}

impl visit::Visitor for ContainsTransitionVisitor {
    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        match stmt.node {
            StmtKind::Mac(ref mac) if is_transition_path(&(*mac).0.node.path) => {
                self.contains_transition = true;
            }
            _ => {
                visit::walk_stmt(self, stmt)
            }
        }
    }

    fn visit_expr(&mut self, expr: &ast::Expr) {
        match expr.node {
            ExprKind::Ret(Some(_)) | ExprKind::Assign(..) => {
                self.contains_transition = true;
            }
            ExprKind::Break(_) if self.inside_loop => {
                self.contains_transition = true;
            }
            ExprKind::Continue(_) if self.inside_loop => {
                self.contains_transition = true;
            }
            ExprKind::Mac(ref mac) if mac.contains_transition(self.inside_loop) => {
                self.contains_transition = true;
            }
            _ => {
                visit::walk_expr(self, expr)
            }
        }
    }

    fn visit_mac(&mut self, _mac: &ast::Mac) { }
}

pub enum Transition {
    Yield(P<ast::Expr>),
    Await(P<ast::Expr>),
}

pub fn parse_mac_transition(cx: &ExtCtxt, mac: &ast::Mac) -> Option<Transition> {
    if is_yield_path(&mac.node.path) {
        Some(Transition::Yield(parse_mac_yield(cx, mac)))
    } else if is_await_path(&mac.node.path) {
        Some(Transition::Await(parse_mac_await(cx, mac)))
    } else {
        None
    }
}

pub fn parse_mac_yield(cx: &ExtCtxt, mac: &ast::Mac) -> P<ast::Expr> {
    let rdr = new_tt_reader(
        &cx.parse_sess().span_diagnostic,
        None,
        None,
        mac.node.tts.clone());

    let mut parser = Parser::new(cx.parse_sess(), cx.cfg(), Box::new(rdr.clone()));
    let expr = panictry!(parser.parse_expr());
    panictry!(parser.expect(&Token::Eof));

    expr
}

pub fn parse_mac_await(cx: &ExtCtxt, mac: &ast::Mac) -> P<ast::Expr> {
    let rdr = new_tt_reader(
        &cx.parse_sess().span_diagnostic,
        None,
        None,
        mac.node.tts.clone());

    let mut parser = Parser::new(cx.parse_sess(), cx.cfg(), Box::new(rdr.clone()));
    let expr = panictry!(parser.parse_expr());
    panictry!(parser.expect(&Token::Eof));

    expr
}

pub fn is_transition_path(path: &ast::Path) -> bool {
    is_yield_path(path) || is_await_path(path)
}

pub fn is_yield_path(path: &ast::Path) -> bool {
    let builder = AstBuilder::new();
    let yield_ = builder.path()
        .id("yield_")
        .build();

    !path.global && path.segments == yield_.segments
}

pub fn is_await_path(path: &ast::Path) -> bool {
    let builder = AstBuilder::new();
    let await = builder.path()
        .id("await")
        .build();

    !path.global && path.segments == await.segments
}
