use inkwell::context::Context;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::values::{FunctionValue, PointerValue, BasicValueEnum, IntValue};
pub struct CodeGenerator<'ctx>{
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
}
