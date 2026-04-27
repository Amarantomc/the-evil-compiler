
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