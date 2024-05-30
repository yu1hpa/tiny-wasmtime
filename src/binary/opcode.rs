use num_derive::FromPrimitive;

#[derive(Debug, FromPrimitive, PartialEq)]
pub enum Opcode {
    End = 0x0B,
    LocalGet = 0x20,
    I64Const = 0x42,
    I32Add = 0x6A,
    I64Add = 0x7C,
}
