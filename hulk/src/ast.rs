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
    TypeDecl(TypeDeclNode),
    Expression(TypedExpr),
}

#[derive(Debug)]
pub struct FunctionDecl {
    pub name: LiteralNode,
    pub params: Vec<(LiteralNode, HulkType)>,
    pub body: TypedExpr,
}

impl FunctionDecl {
    pub fn new(name: LiteralNode, params: Vec<(LiteralNode,HulkType)>, body: TypedExpr) -> Self {
        FunctionDecl { name, params, body }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HulkType {
    Number,
    Bool,
    String,
    Class(String),
    Unknown,
}


#[derive(Debug)]
pub struct TypedExpr {
    pub kind: Expr,
    pub return_type: HulkType,
}

impl TypedExpr {
    pub fn new(kind: Expr) -> Self {
        TypedExpr {
            kind,
            return_type: HulkType::Unknown, // Por defecto es desconocido hasta la fase semántica
        }
    }
    
    // Opcional: un constructor si ya sabes el tipo desde el parseo
    pub fn with_type(kind: Expr, return_type: HulkType) -> Self {
        TypedExpr { kind, return_type }
    }

    pub fn accept<T>(&self, v: &mut impl ExprVisitor<T>) -> T {
        match &self.kind {
            Expr::Literal(node) => match &node.value {
                Literal::Number(n) => v.visit_number(*n),
                Literal::Bool(b) => v.visit_bool(*b),
                Literal::Str(s) => v.visit_string(s),
                Literal::Id(id) => v.visit_id(id),
            },
            Expr::Binary(node) => v.visit_binary_op(&node.left, &node.op, &node.right),
            Expr::Unary(node) => v.visit_unary_op(&node.op, &node.expr),
            Expr::Let(node) => v.visit_let(node),
            Expr::If(node) => v.visit_if(node),
            Expr::While(node) => v.visit_while(node),
            Expr::For(node) => v.visit_for(node),
            Expr::FunCall(node) => v.visit_fun_call(node),
            Expr::DestAssign(node) => v.visit_dest_assign(node),
            Expr::Block(node) => v.visit_block(node),
            Expr::Instantiation(node) => v.visit_instantiation(node),
            Expr::MemberAccess(node) => v.visit_member_access(node),
            Expr::MethodCall(node) => v.visit_method_call(node),
        }
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
    Block(BlockNode),
    Instantiation(InstantiationNode),
    MemberAccess(MemberAccessNode),
    MethodCall(MethodCallNode),
}

#[derive(Debug)]
pub struct MemberAccessNode {
    pub instance: Box<TypedExpr>,
    pub member: LiteralNode,
}

#[derive(Debug)]
pub struct MethodCallNode {
    pub instance: Box<TypedExpr>,
    pub call: FunCallNode,
}

pub type InstantiationNode = FunCallNode;

#[derive(Debug)]
pub struct TypeDeclNode {
    pub name: LiteralNode,
    pub params: Vec<(LiteralNode, HulkType)>,
    pub attributes: Vec<AttributeNode>,
    pub methods: Vec<FunctionDecl>,
}

#[derive(Debug)]
pub struct AttributeNode {
    pub name: LiteralNode,
    pub type_annotation: HulkType,
    pub initializer: TypedExpr,
}

#[derive(Debug)]
pub struct LetNode {
    pub assignments: Vec<((LiteralNode,HulkType), TypedExpr)>,
    pub body: Box<TypedExpr>,
}

impl LetNode {
    pub fn new(assignments: Vec<((LiteralNode,HulkType), TypedExpr)>, body: TypedExpr) -> Self {
        LetNode {
            assignments,
            body: Box::new(body),
        }
    }
}

#[derive(Debug)]
pub struct IfNode {
    pub condition: Box<TypedExpr>,
    pub if_branch: Box<TypedExpr>,
    pub elif_branches: Vec<(TypedExpr, TypedExpr)>,
    pub else_branch: Box<TypedExpr>,
}

impl IfNode {
    pub fn new(condition: TypedExpr, if_branch:TypedExpr, elif_branches: Vec<(TypedExpr, TypedExpr)>, else_branch: TypedExpr) -> Self {
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
    pub condition: Box<TypedExpr>,
    pub body: Box<TypedExpr>,
}

impl WhileNode {
    pub fn new(condition: TypedExpr, body: TypedExpr) -> Self {
        WhileNode {
            condition: Box::new(condition),
            body: Box::new(body),
        }
    }
}

#[derive(Debug)]
pub struct ForNode {
    pub variable: LiteralNode,
    pub iterator: Box<TypedExpr>,
    pub body: Box<TypedExpr>,
}

impl ForNode {
    pub fn new(variable: LiteralNode, iterator: TypedExpr, body: TypedExpr) -> Self {
        ForNode {
            variable,
            iterator: Box::new(iterator),
            body: Box::new(body),
        }
    }
}

#[derive(Debug)]
pub struct FunCallNode {
    pub name: LiteralNode,
    pub args: Vec<TypedExpr>,
}

impl FunCallNode {
    pub fn new(name: LiteralNode, args: Vec<TypedExpr>) -> Self {
        FunCallNode { name, args }
    }
}

#[derive(Debug)]
pub struct DestAssignNode {
    pub identifier: LiteralNode,
    pub expr: Box<TypedExpr>,
}

impl DestAssignNode {
    pub fn new(identifier: LiteralNode, expr: TypedExpr) -> Self {
        DestAssignNode {
            identifier,
            expr: Box::new(expr),
        }
    }
}

#[derive(Debug)]
pub struct BinaryOpNode {
    pub left: Box<TypedExpr>,
    pub op: BinaryOp,
    pub right: Box<TypedExpr>,
}

impl BinaryOpNode {
    pub fn new(left: TypedExpr, op: BinaryOp, right: TypedExpr) -> Self {
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
    pub expr: Box<TypedExpr>,
}

impl UnaryOpNode {
    pub fn new(op: UnaryOp, expr: TypedExpr) -> Self {
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
pub struct BlockNode {
    pub expressions: Vec<TypedExpr>,
}

impl BlockNode {
    pub fn new(expressions: Vec<TypedExpr>) -> Self {
        BlockNode { expressions }
    }
}

#[derive(Debug)]
pub enum Literal {
    Number(f32),
    Bool(bool),
    Str(String),
    Id(String)
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

// impl Expr {
  
// }
