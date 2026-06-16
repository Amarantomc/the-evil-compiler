use crate::{expr_visitor::ExprVisitor, nodes::{
    binaryop_node::BinaryOpNode,
    block_node::BlockNode,
    destassing_node::DestAssignNode,
    for_node::ForNode,
    funcall_node::FunCallNode,
    if_node::IfNode,
    instantiation_node::InstantiationNode,
    let_node::LetNode,
    literal_node::{Literal, LiteralNode},
    member_access_node::{MemberAccessNode, MethodCallNode},
    tuple_node::{TupleNode, TupleAccessNode},
    type_downcast_node::TypeDowncastNode,
    type_test_node::TypeTestNode,
    unaryop_node::UnaryOpNode,
    while_node::WhileNode,
}};


#[derive(Debug, Clone, PartialEq)]
pub enum HulkType {
    Number,
    Bool,
    String,
    Class(String),
    Tuple(Vec<HulkType>),
    Unknown,
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
    SelfRef,
    BaseCall(Vec<Expr>),
    TypeDowncast(TypeDowncastNode),
    TypeTest(TypeTestNode),
    Tuple(TupleNode),
    TupleAccess(TupleAccessNode),
}




 




impl Expr {
     
    pub fn accept<T>(&mut self, v: &mut impl ExprVisitor<T>) -> T {
        match self {
            Expr::Literal(node) => match &node.value {
                Literal::Number(n) => v.visit_number(*n),
                Literal::Bool(b) => v.visit_bool(*b),
                Literal::Str(s) => v.visit_string(s),
                Literal::Id(id) => v.visit_id(id),
            },
            Expr::SelfRef => v.visit_self(),
            Expr::Binary(node) => v.visit_binary_op(&mut node.left, &node.op, &mut node.right),
            Expr::Unary(node) => v.visit_unary_op(&node.op, &mut node.expr),
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
            Expr::BaseCall(typed_exprs) => v.visit_base_call(typed_exprs),
            Expr::TypeDowncast(type_downcast_node) => v.visit_type_downcast(type_downcast_node),
            Expr::TypeTest(type_test_node) => v.visit_type_test(type_test_node),
            Expr::Tuple(tuple_node) => v.visit_tuple(tuple_node),
            Expr::TupleAccess(tuple_access_node) => v.visit_tuple_access(tuple_access_node),
        }
    }
}

