use aster::AstBuilder;

use petgraph::EdgeDirection;
use petgraph::graph::{self, Graph, NodeIndex};

use syntax::ast;
use syntax::codemap::Span;
use syntax::visit;
use syntax::ptr::P;

//////////////////////////////////////////////////////////////////////////////

pub struct CFGBuilder {
    graph: Graph<Node, ()>,
    inside_loop: bool,
}

impl CFGBuilder {
    pub fn new() -> Self {
        CFGBuilder {
            graph: Graph::new(),
            inside_loop: false,
        }
    }

    pub fn build(mut self, fn_decl: &ast::FnDecl, block: &ast::Block) -> CFG {
        let mut scope = Vec::new();

        // The initial scope is the function scope arguments.
        for arg in fn_decl.inputs.iter() {
            scope.extend(self.find_decl_idents(&arg.pat));
        }

        let entry = self.add_bb("Entry", &scope);
        let exit = self.graph.add_node(Node::Exit);

        self.block(entry, exit, &scope, block);

        CFG {
            graph: self.graph,
            entry: entry,
            exit: exit,
        }
    }

    fn block(&mut self,
             pred: NodeIndex,
             next: NodeIndex,
             scope: &Vec<ast::Ident>,
             block: &ast::Block) -> NodeIndex {
        let (_, pred) = self.block_inner(block, pred, next, scope);

        let exit = self.add_bb("BlockExit", &scope);
        self.goto(pred, exit)
    }

    fn block_inner(&mut self,
                   block: &ast::Block,
                   mut pred: NodeIndex,
                   next: NodeIndex,
                   parent_scope: &Vec<ast::Ident>) -> (NodeIndex, NodeIndex) {
        // Create a new scope so that all our declarations will be dropped when it goes out of
        // bounds.
        let mut scope = parent_scope.clone();

        let entry = pred;

        for stmt in block.stmts.iter() {
            pred = self.stmt(pred, next, &mut scope, stmt);
        }

        if block.expr.is_some() {
            panic!("cannot handle block expressions yet");
        }

        (entry, pred)
    }

    fn add_edge(&mut self, src: NodeIndex, dst: NodeIndex) {
        self.graph.add_edge(src, dst, ());
    }

    /*
    fn return_(&mut self, src: NodeIndex, name: String, expr: P<ast::Expr>) -> NodeIndex {
        self.add_edge(src, Edge::Return {
            name: name,
            expr: expr,
        })
    }
    */

    fn goto(&mut self,
            src: NodeIndex,
            dst: NodeIndex) -> NodeIndex {
        self.add_edge(src, dst);
        self.add_stmt(src, Stmt::Goto(dst));

        dst
    }

    fn yield_(&mut self,
              src: NodeIndex,
              expr: &P<ast::Expr>,
              scope: &Vec<ast::Ident>) -> NodeIndex {
        let dst = self.add_bb("Yield", scope);
        self.add_edge(src, dst);
        self.add_stmt(src, Stmt::Yield(dst, expr.clone()));

        dst
    }

    fn add_stmt(&mut self, nx: NodeIndex, stmt: Stmt) {
        let bb = self.get_node_mut(nx);
        bb.stmts.push(stmt);
    }

    fn stmt(&mut self,
            pred: NodeIndex,
            next: NodeIndex,
            scope: &mut Vec<ast::Ident>,
            stmt: &P<ast::Stmt>) -> NodeIndex {
        match stmt.node {
            ast::Stmt_::StmtDecl(ref decl, _) => {
                match decl.node {
                    ast::Decl_::DeclLocal(ref local) => {
                        scope.extend(self.find_decl_idents(&local.pat));
                    }
                    _ => {
                        panic!("cannot handle item declarations yet");
                    }
                }

                self.add_stmt(pred, Stmt::Stmt(stmt.clone()));
                pred
            }
            ast::Stmt_::StmtSemi(ref expr, _) if self.contains_transition_expr(expr) => {
                self.stmt_semi(pred, next, &*scope, expr)
            }
            _ => {
                self.add_stmt(pred, Stmt::Stmt(stmt.clone()));
                pred
            }
        }
    }

    fn stmt_semi(&mut self,
                 pred: NodeIndex,
                 next: NodeIndex,
                 scope: &Vec<ast::Ident>,
                 expr: &P<ast::Expr>) -> NodeIndex {
        match expr.node {
            ast::Expr_::ExprRet(Some(ref expr)) => {
                self.yield_(pred, expr, scope)
            }
            ast::Expr_::ExprRet(None) => {
                panic!("cannot handle empty returns yet");
            }
            ast::Expr_::ExprBreak(None) => {
                self.goto(pred, next)
            }
            ast::Expr_::ExprBlock(ref block) => {
                self.block(pred, next, scope, block)
            }
            ast::Expr_::ExprLoop(ref block, _) => {
                let old_inside_loop = self.inside_loop;
                self.inside_loop = true;

                let pred = self.expr_loop(pred, next, scope, block);

                self.inside_loop = old_inside_loop;
                pred
            }
            ast::Expr_::ExprIf(ref expr, ref then, ref else_) => {
                self.expr_if(pred, next, scope, expr, then, else_)
            }
            ref expr => {
                panic!("cannot handle {:?} yet", expr);
            }
        }
    }

    fn expr_loop(&mut self,
                 pred: NodeIndex,
                 next: NodeIndex,
                 scope: &Vec<ast::Ident>,
                 block: &ast::Block) -> NodeIndex {
        let (entry, pred) = self.block_inner(block, pred, next, scope);
        self.goto(pred, entry);

        pred
    }

    fn expr_if(&mut self,
               pred: NodeIndex,
               next: NodeIndex,
               scope: &Vec<ast::Ident>,
               expr: &P<ast::Expr>,
               then: &P<ast::Block>,
               else_: &Option<P<ast::Expr>>) -> NodeIndex {
        assert!(!self.contains_transition_expr(expr));
        assert!(then.expr.is_none());

        let builder = AstBuilder::new();

        let then_nx = self.add_bb("Then", scope);
        let else_nx = self.add_bb("Else", scope);
        let endif_nx = self.add_bb("EndIf", scope);

        self.add_stmt(pred, Stmt::If(expr.clone(), then_nx, else_nx));
        self.add_edge(pred, then_nx);
        self.add_edge(pred, else_nx);

        let (_, pred) = self.block_inner(then, then_nx, next, scope);
        //self.goto(pred, endif_nx);

        let else_ = match *else_ {
            Some(ref else_) => {
                builder.block()
                    .stmt().semi().build(else_.clone())
                    .build()
            }
            None => {
                builder.block().build()
            }
        };

        self.block_inner(&else_, else_nx, next, scope);

        endif_nx
    }

    fn add_bb<T>(&mut self, name: T, scope: &Vec<ast::Ident>) -> NodeIndex
        where T: Into<String>
    {
        let name = name.into();
        let bb = BasicBlock::new(name, scope.clone());

        self.graph.add_node(Node::BasicBlock(bb))
    }

    fn get_node_mut(&mut self, index: NodeIndex) -> &mut BasicBlock {
        match self.graph.node_weight_mut(index) {
            Some(node) => {
                match *node {
                    Node::BasicBlock(ref mut bb) => bb,
                    ref node => {
                        panic!("node is not a basic block: {:?}", node)
                    }
                }
            }
            None => {
                panic!("missing node!")
            }
        }
    }

    fn find_decl_idents(&self, pat: &ast::Pat) -> Vec<ast::Ident> {
        struct Visitor(Vec<ast::Ident>);

        impl<'a> visit::Visitor<'a> for Visitor {
            fn visit_ident(&mut self, _span: Span, ident: ast::Ident) {
                self.0.push(ident);
            }
        }

        let mut visitor = Visitor(Vec::new());
        visit::Visitor::visit_pat(&mut visitor, pat);
        visitor.0
    }

    fn contains_transition_expr(&self, expr: &ast::Expr) -> bool {
        struct Visitor {
            contains_transition: bool,
            inside_loop: bool,
        }

        impl<'a> visit::Visitor<'a> for Visitor {
            fn visit_expr(&mut self, expr: &ast::Expr) {
                match expr.node {
                    ast::Expr_::ExprRet(Some(_)) => {
                        self.contains_transition = true;
                    }
                    ast::Expr_::ExprBreak(_) if self.inside_loop => {
                        self.contains_transition = true;
                    }
                    ast::Expr_::ExprAgain(_) if self.inside_loop => {
                        self.contains_transition = true;
                    }
                    _ => {
                        visit::walk_expr(self, expr)
                    }
                }
            }
        }

        let mut visitor = Visitor {
            contains_transition: false,
            inside_loop: self.inside_loop,
        };

        visit::Visitor::visit_expr(&mut visitor, expr);
        visitor.contains_transition
    }
}

//////////////////////////////////////////////////////////////////////////////

pub struct CFG {
    pub graph: Graph<Node, ()>,
    pub entry: NodeIndex,
    pub exit: NodeIndex,
}

impl CFG {
    pub fn get_node(&self, nx: NodeIndex) -> &Node {
        &self.graph[nx]
    }

    pub fn get_node_decls(&self, nx: NodeIndex) -> &[ast::Ident] {
        self.get_node(nx).decls()
    }

    pub fn get_edges(&self, nx: NodeIndex, direction: EdgeDirection) -> graph::Edges<()> {
        self.graph.edges_directed(nx, direction)
    }

    pub fn get_child_edges(&self, nx: NodeIndex) -> graph::Edges<()> {
        self.get_edges(nx, EdgeDirection::Outgoing)
    }
}

//////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
pub enum Node {
    BasicBlock(BasicBlock),
    //Placeholder(String),
    Exit,
}

impl Node {
    pub fn name(&self) -> &str {
        match *self {
            Node::BasicBlock(ref bb) => &bb.name[..],
            //Node::Placeholder(_) => panic!("placeholder"),
            Node::Exit => "Exit",
        }
    }

    pub fn decls(&self) -> &[ast::Ident] {
        match *self {
            Node::BasicBlock(ref bb) => &bb.decls[..],
            //Node::Placeholder(_) => panic!("placeholder"),
            Node::Exit => &[],
        }
    }
}

#[derive(Debug)]
pub struct BasicBlock {
    name: String,
    decls: Vec<ast::Ident>,
    pub stmts: Vec<Stmt>,
}

impl BasicBlock {
    fn new(name: String, decls: Vec<ast::Ident>) -> Self {
        BasicBlock {
            name: name,
            decls: decls,
            stmts: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum Stmt {
    Stmt(P<ast::Stmt>),
    Goto(NodeIndex),
    Yield(NodeIndex, P<ast::Expr>),
    If(P<ast::Expr>, NodeIndex, NodeIndex),
}
