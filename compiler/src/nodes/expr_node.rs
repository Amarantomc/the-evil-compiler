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
    Param(String),
    Generic(String, Vec<HulkType>),
    Unknown,
}
use std::collections::HashMap;

impl HulkType {
    /// Reescribe Class(n) -> Param(n) para cada n declarado como genérico.
    /// Aplica recursivamente a tuplas y genéricos aplicados.
    pub fn promote_params(&self, generics: &[String]) -> HulkType {
        match self {
            HulkType::Class(n) if generics.iter().any(|g| g == n) =>
                HulkType::Param(n.clone()),
            HulkType::Tuple(es) =>
                HulkType::Tuple(es.iter().map(|e| e.promote_params(generics)).collect()),
            HulkType::Generic(n, args) =>
                HulkType::Generic(n.clone(),
                    args.iter().map(|a| a.promote_params(generics)).collect()),
            other => other.clone(),
        }
    }

    /// Sustituye Param(n) por el tipo concreto del mapa. Recursa en tuplas/genéricos.
    pub fn subst(&self, map: &HashMap<String, HulkType>) -> HulkType {
        match self {
            HulkType::Param(n) => map.get(n).cloned().unwrap_or_else(|| self.clone()),
            HulkType::Tuple(es) =>
                HulkType::Tuple(es.iter().map(|e| e.subst(map)).collect()),
            HulkType::Generic(n, args) => {
                let new_args: Vec<HulkType> = args.iter().map(|a| a.subst(map)).collect();
                // Si ya no quedan Param dentro, lo dejamos como Generic concreto:
                // el monomorfizador lo manglará a Class("n__args").
                HulkType::Generic(n.clone(), new_args)
            }
            other => other.clone(),
        }
    }

    pub fn contains_param(&self) -> bool {
        match self {
            HulkType::Param(_) => true,
            HulkType::Tuple(es) => es.iter().any(|t| t.contains_param()),
            HulkType::Generic(_, args) => args.iter().any(|t| t.contains_param()),
            _ => false,
        }
    }

    /// Nombre plano apto para identificadores LLVM (mangling).
    pub fn mangle(&self) -> String {
        match self {
            HulkType::Number      => "Number".into(),
            HulkType::Bool        => "Bool".into(),
            HulkType::String      => "String".into(),
            HulkType::Class(n)    => n.clone(),
            HulkType::Tuple(es)   => format!(
                "Tup{}_{}", es.len(),
                es.iter().map(|t| t.mangle()).collect::<Vec<_>>().join("_")),
            HulkType::Generic(n, a) => format!(
                "{}__{}", n,
                a.iter().map(|t| t.mangle()).collect::<Vec<_>>().join("_")),
            HulkType::Param(n)    => n.clone(),
            HulkType::Unknown     => "Unknown".into(),
        }
    }

    /// Colapsa Generic(n,args) concreto a Class("n__args") (post-sustitución).
    pub fn collapse_generic(&self) -> HulkType {
        match self {
            HulkType::Generic(_, _) => HulkType::Class(self.mangle()),
            HulkType::Tuple(es) =>
                HulkType::Tuple(es.iter().map(|e| e.collapse_generic()).collect()),
            other => other.clone(),
        }
    }
    
}

// Representa todo tipo de expresión en el lenguaje
#[derive(Debug, Clone)]
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
fn merge_span(a: (usize, usize), b: (usize, usize)) -> (usize, usize) {
    let start = match (a.0, b.0) {
        (0, s) => s,
        (s, 0) => s,
        (x, y) => x.min(y),
    };
    (start, a.1.max(b.1))
}

impl Expr {
    /// Posición `(inicio, fin)` en bytes de esta expresión, derivada
    /// estructuralmente de sus hojas (los literales llevan span del lexer).
    /// Devuelve `(0,0)` cuando no hay posición disponible.
    pub fn span(&self) -> (usize, usize) {
        match self {
            Expr::Literal(n) => n.span,
            Expr::SelfRef => (0, 0),
            Expr::Binary(n) => merge_span(n.left.span(), n.right.span()),
            Expr::Unary(n) => n.expr.span(),
            Expr::Let(n) => {
                let start = n.assignments.first()
                    .map(|((id, _), _)| id.span)
                    .unwrap_or((0, 0));
                merge_span(start, n.body.span())
            }
            Expr::If(n) => merge_span(n.condition.span(), n.else_branch.span()),
            Expr::While(n) => merge_span(n.condition.span(), n.body.span()),
            Expr::For(n) => merge_span(n.variable.span, n.body.span()),
            Expr::Block(n) => {
                let first = n.expressions.first().map(|e| e.span()).unwrap_or((0, 0));
                let last  = n.expressions.last().map(|e| e.span()).unwrap_or((0, 0));
                merge_span(first, last)
            }
            Expr::FunCall(n) => n.name.span,
            Expr::Instantiation(n) => n.name.span,
            Expr::DestAssign(n) => merge_span(n.target.span(), n.expr.span()),
            Expr::MemberAccess(n) => merge_span(n.instance.span(), n.member.span),
            Expr::MethodCall(n) => merge_span(n.instance.span(), n.call.name.span),
            Expr::BaseCall(args) => {
                let first = args.first().map(|e| e.span()).unwrap_or((0, 0));
                let last  = args.last().map(|e| e.span()).unwrap_or((0, 0));
                merge_span(first, last)
            }
            Expr::TypeDowncast(n) => merge_span(n.expr.span(), n.target_type.span),
            Expr::TypeTest(n) => merge_span(n.expr.span(), n.target_type.span),
            Expr::Tuple(n) => {
                let first = n.elements.first().map(|e| e.span()).unwrap_or((0, 0));
                let last  = n.elements.last().map(|e| e.span()).unwrap_or((0, 0));
                merge_span(first, last)
            }
            Expr::TupleAccess(n) => n.tuple.span(),
        }
    }
     
      pub fn static_type(&self) -> HulkType {
        match self {
            Expr::Binary(n) => n.return_type.clone(),
            Expr::Unary(n) => n.return_type.clone(),
            Expr::Let(n) => n.return_type.clone(),
            Expr::If(n) => n.return_type.clone(),
            Expr::While(n) => n.return_type.clone(),
            Expr::For(n) => n.return_type.clone(),
            Expr::Block(n) => n.return_type.clone(),
            Expr::FunCall(n) => n.return_type.clone(),
            Expr::Instantiation(n) => n.return_type.clone(),
            Expr::DestAssign(n) => n.return_type.clone(),
            Expr::MemberAccess(n) => n.return_type.clone(),
            Expr::MethodCall(n) => n.return_type.clone(),
            Expr::TypeDowncast(n) => n.return_type.clone(),
            Expr::TypeTest(n) => n.return_type.clone(),
            Expr::Tuple(n) => n.return_type.clone(),
            Expr::TupleAccess(n) => n.return_type.clone(),
            Expr::Literal(_) | Expr::SelfRef | Expr::BaseCall(_) => HulkType::Unknown,
        }
    }
}
