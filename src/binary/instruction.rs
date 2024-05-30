#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    End,
    LocalGet(u32),
    I64Const(i64),
    I32Add,
    I64Add,
}
