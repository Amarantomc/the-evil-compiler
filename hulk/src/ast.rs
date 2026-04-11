use crate::expr_visitor::ExprVisitor;

#[derive(Debug)]
pub struct Program {
    pub statements: Vec<Statement>,
}

impl Program {
    pub fn new(statements: Vec<Statement>) -> Self {
        Program { statements }
    }
}

#[derive(Debug)]
pub enum Statement {
    FunctionDecl(FunctionDecl),
    Expression(Expr),
}

#[derive(Debug)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<String>,
    pub body: Expr,
}

impl FunctionDecl {
    pub fn new(name: String, params: Vec<String>, body: Expr) -> Self {
        FunctionDecl { name, params, body }
    }
}

// Representa todo tipo de expresión en el lenguaje
#[derive(Debug)]
pub enum Expr {
    Let(LetNode),
    If(IfNode),
    While(WhileNode),
    For(ForNode),
    FunCall(FunCallNode),
    DestAssign(DestAssignNode),
    Binary(BinaryOpNode),
    Unary(UnaryOpNode),
    Literal(LiteralNode),
    Identifier(IdentifierNode),
    Block(BlockNode),
}

#[derive(Debug)]
pub struct LetNode {
    pub assignments: Vec<(String, Expr)>,
    pub body: Box<Expr>,
}

impl LetNode {
    pub fn new(assignments: Vec<(String, Expr)>, body: Expr) -> Self {
        LetNode {
            assignments,
            body: Box::new(body),
        }
    }
}

#[derive(Debug)]
pub struct IfNode {
    pub condition: Box<Expr>,
    pub if_branch: Box<Expr>,
    pub elif_branches: Vec<(Expr, Expr)>,
    pub else_branch: Box<Expr>,
}

impl IfNode {
    pub fn new(condition: Expr, if_branch: Expr, elif_branches: Vec<(Expr, Expr)>, else_branch: Expr) -> Self {
        IfNode {
            condition: Box::new(condition),
            if_branch: Box::new(if_branch),
            elif_branches,
            else_branch: Box::new(else_branch),
        }
    }
}

#[derive(Debug)]
pub struct WhileNode {
    pub condition: Box<Expr>,
    pub body: Box<Expr>,
}

impl WhileNode {
    pub fn new(condition: Expr, body: Expr) -> Self {
        WhileNode {
            condition: Box::new(condition),
            body: Box::new(body),
        }
    }
}

#[derive(Debug)]
pub struct ForNode {
    pub variable: String,
    pub iterator: Box<Expr>,
    pub body: Box<Expr>,
}

impl ForNode {
    pub fn new(variable: String, iterator: Expr, body: Expr) -> Self {
        ForNode {
            variable,
            iterator: Box::new(iterator),
            body: Box::new(body),
        }
    }
}

#[derive(Debug)]
pub struct FunCallNode {
    pub name: String,
    pub args: Vec<Expr>,
}

impl FunCallNode {
    pub fn new(name: String, args: Vec<Expr>) -> Self {
        FunCallNode { name, args }
    }
}

#[derive(Debug)]
pub struct DestAssignNode {
    pub identifier: String,
    pub expr: Box<Expr>,
}

impl DestAssignNode {
    pub fn new(identifier: String, expr: Expr) -> Self {
        DestAssignNode {
            identifier,
            expr: Box::new(expr),
        }
    }
}

#[derive(Debug)]
pub struct BinaryOpNode {
    pub left: Box<Expr>,
    pub op: BinaryOp,
    pub right: Box<Expr>,
}

impl BinaryOpNode {
    pub fn new(left: Expr, op: BinaryOp, right: Expr) -> Self {
        BinaryOpNode {
            left: Box::new(left),
            op,
            right: Box::new(right),
        }
    }
}

#[derive(Debug)]
pub struct UnaryOpNode {
    pub op: UnaryOp,
    pub expr: Box<Expr>,
}

impl UnaryOpNode {
    pub fn new(op: UnaryOp, expr: Expr) -> Self {
        UnaryOpNode {
            op,
            expr: Box::new(expr),
        }
    }
}

#[derive(Debug)]
pub struct LiteralNode {
    pub value: Literal,
}

impl LiteralNode {
    pub fn new(value: Literal) -> Self {
        LiteralNode { value }
    }
}

#[derive(Debug)]
pub struct IdentifierNode {
    pub name: String,
}

impl IdentifierNode {
    pub fn new(name: String) -> Self {
        IdentifierNode { name }
    }
}

#[derive(Debug)]
pub struct BlockNode {
    pub expressions: Vec<Expr>,
}

impl BlockNode {
    pub fn new(expressions: Vec<Expr>) -> Self {
        BlockNode { expressions }
    }
}

#[derive(Debug)]
pub enum Literal {
    Number(f32),
    Bool(bool),
    Str(String),
}

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Mul,
    Div,
    Add,
    Sub,
    Pow,
    Equal,
    Dist,
    Gequa,
    Lequa,
    Great,
    Less,
    And,
    Or,
    Mod,
}
#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
    Plus,
}

impl Expr {
    pub fn accept<T>(&self, v: &mut impl ExprVisitor<T>) -> T {
        match self {
            Expr::Literal(node) => match &node.value {
                Literal::Number(n) => v.visit_number(*n),
                _ => todo!("Implement visit for other literals"),
            },
            Expr::Binary(node) => v.visit_binary_op(&node.left, &node.op, &node.right),
            _ => todo!("Implement visit for other expression types"),
        }
    }
}
