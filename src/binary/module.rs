use super::{
    instruction::Instruction,
    opcode::Opcode,
    section::{Function, SectionCode},
    types::{Export, ExportDesc, FuncType, FunctionLocal, ValueType},
};
use nom::{
    bytes::complete::{tag, take},
    multi::many0,
    number::complete::{le_u32, le_u8},
    IResult,
};
use nom_leb128::{leb128_i64, leb128_u32};
use num_traits::FromPrimitive as _;

#[derive(Debug, PartialEq, Eq)]
pub struct Module {
    pub magic: String,
    pub version: u32,
    pub type_section: Option<Vec<FuncType>>,
    pub function_section: Option<Vec<u32>>,
    pub code_section: Option<Vec<Function>>,
    pub export_section: Option<Vec<Export>>,
}

impl Default for Module {
    fn default() -> Self {
        Self {
            magic: "\0asm".to_string(),
            version: 1,
            type_section: None,
            function_section: None,
            code_section: None,
            export_section: None,
        }
    }
}

impl Module {
    pub fn new(input: &[u8]) -> anyhow::Result<Module> {
        let (_, module) =
            Module::decode(input).map_err(|e| anyhow::anyhow!("failed to parse wasm: {}", e))?;
        Ok(module)
    }

    fn decode(input: &[u8]) -> IResult<&[u8], Module> {
        let (input, _) = tag(b"\0asm")(input)?;
        let (input, version) = le_u32(input)?;
        let mut module = Module {
            magic: "\0asm".to_string(),
            version,
            ..Default::default()
        };

        let mut remaining = input;
        while !remaining.is_empty() {
            match decode_section_header(remaining) {
                Ok((input, (code, size))) => {
                    println!("[+] (1) 各Sectionのサイズ: {:?}", size); //(1)

                    // 指定したサイズ分だけ読み取る
                    let (rest, section_contents) = take(size)(input)?;

                    match code {
                        SectionCode::Type => {
                            let (_, types) = decode_type_section(section_contents)?;
                            module.type_section = Some(types);
                        }
                        SectionCode::Function => {
                            let (_, func_idx_list) = decode_function_section(section_contents)?;
                            module.function_section = Some(func_idx_list);
                        }
                        SectionCode::Code => {
                            let (_, funcs) = decode_code_section(section_contents)?;
                            module.code_section = Some(funcs);
                        }
                        SectionCode::Export => {
                            let (_, exports) = decode_export_section(section_contents)?;
                            module.export_section = Some(exports);
                        }
                        _ => todo!(),
                    };
                    remaining = rest;
                }
                Err(err) => return Err(err),
            }
        }
        Ok((input, module))
    }
}

fn decode_section_header(input: &[u8]) -> IResult<&[u8], (SectionCode, u32)> {
    let (input, code) = le_u8(input)?;
    let (input, size) = leb128_u32(input)?;

    Ok((
        input,
        (
            SectionCode::from_u8(code).expect("unexpected section code"),
            size,
        ),
    ))
}

fn decode_value_type(input: &[u8]) -> IResult<&[u8], ValueType> {
    let (input, value_type) = le_u8(input)?;
    Ok((input, value_type.into()))
}

fn decode_type_section(input: &[u8]) -> IResult<&[u8], Vec<FuncType>> {
    let mut func_types = vec![];
    let (mut input, count) = leb128_u32(input)?;

    // 関数シグネチャの個数分、読み取る
    for _ in 0..count {
        let (rest, _) = le_u8(input)?; // 関数シグネチャの種類を表す値(0x60)
        let mut func = FuncType::default();

        // 引数の個数を読み取る
        let (rest, size) = leb128_u32(rest)?;
        // 引数の型を読み取る
        let (rest, types) = take(size)(rest)?;
        // 引数の型をu8からValueTypeに変換
        let (_, types) = many0(decode_value_type)(types)?;
        func.params = types;

        // 戻り値の個数を読み取る
        let (rest, size) = leb128_u32(rest)?;
        let (rest, types) = take(size)(rest)?;
        let (_, types) = many0(decode_value_type)(types)?;
        func.results = types;

        func_types.push(func);
        input = rest;
    }

    Ok((&[], func_types))
}

fn decode_function_section(input: &[u8]) -> IResult<&[u8], Vec<u32>> {
    let mut func_idx_list = vec![];
    let (mut input, count) = leb128_u32(input)?;

    for _ in 0..count {
        let (rest, idx) = leb128_u32(input)?;
        func_idx_list.push(idx);
        input = rest;
    }

    Ok((&[], func_idx_list))
}

fn decode_code_section(input: &[u8]) -> IResult<&[u8], Vec<Function>> {
    let mut functions = vec![];
    let (mut input, count) = leb128_u32(input)?; // 関数の個数

    for _ in 0..count {
        let (rest, size) = leb128_u32(input)?; // func body size
        println!("[+] (2) 関数のサイズ: {:?}", size); // (2)
        let (rest, body) = take(size)(rest)?;
        let (_, body) = decode_function_body(body)?;
        functions.push(body);
        input = rest;
    }

    Ok((&[], functions))
}

fn decode_function_body(input: &[u8]) -> IResult<&[u8], Function> {
    let mut body = Function::default();

    let (mut input, count) = leb128_u32(input)?; // ローカル変数の個数

    for _ in 0..count {
        // 型の数
        let (rest, type_count) = leb128_u32(input)?;
        // 型
        let (rest, value_type) = le_u8(rest)?;
        body.locals.push(FunctionLocal {
            type_count,
            value_type: value_type.into(),
        });
        input = rest;
    }

    let mut remaining = input;

    while !remaining.is_empty() {
        let (rest, inst) = decode_instructions(remaining)?;
        body.code.push(inst);
        remaining = rest;
    }

    Ok((&[], body))
}

fn decode_instructions(input: &[u8]) -> IResult<&[u8], Instruction> {
    let (input, byte) = le_u8(input)?;
    let op = Opcode::from_u8(byte)
        .unwrap_or_else(|| panic!("invalid or unimplemented opcode: {:X}", byte));

    let (rest, inst) = match op {
        Opcode::End => (input, Instruction::End),
        Opcode::LocalGet => {
            let (rest, idx) = leb128_u32(input)?;
            (rest, Instruction::LocalGet(idx))
        }
        Opcode::I64Const => {
            println!("[+] (3) i64.const 検出"); // (3)

            let (rest, val) = leb128_i64(input)?;
            (rest, Instruction::I64Const(val))
        }
        Opcode::I32Add => (input, Instruction::I32Add),
        Opcode::I64Add => (input, Instruction::I64Add),
    };

    Ok((rest, inst))
}

fn decode_export_section(input: &[u8]) -> IResult<&[u8], Vec<Export>> {
    // エクスポートの要素数
    let (mut input, count) = leb128_u32(input)?;
    let mut exports = vec![];

    for _ in 0..count {
        // エクスポート名のバイト列の長さ
        let (rest, name_len) = leb128_u32(input)?;
        // バイト列の長さ分だけ読み取る
        let (rest, name_bytes) = take(name_len)(rest)?;
        // バイト列を文字列に変換
        let name = String::from_utf8(name_bytes.to_vec()).expect("invalid utf-8 string");

        // エクスポートの種類
        let (rest, export_kind) = le_u8(rest)?;
        // 実態へのインデックス
        let (rest, idx) = leb128_u32(rest)?;

        let desc = match export_kind {
            0x00 => ExportDesc::Func(idx),
            _ => unimplemented!("unsupported export kind: {:x}", export_kind),
        };

        exports.push(Export { name, desc });
        input = rest;
    }
    Ok((input, exports))
}

#[cfg(test)]
mod tests {
    use std::vec;

    use crate::binary::{
        instruction::Instruction,
        module::Module,
        section::Function,
        types::{Export, ExportDesc, FuncType, FunctionLocal, ValueType},
    };
    use anyhow::Result;

    #[test]
    fn decode_simplest_module() -> Result<()> {
        let wasm = wat::parse_str("(module)")?;
        let module = Module::new(&wasm)?;
        assert_eq!(module, Module::default());
        Ok(())
    }

    #[test]
    fn decode_simplest_func() -> Result<()> {
        let wasm = wat::parse_str("(module (func))")?;
        let module = Module::new(&wasm)?;
        assert_eq!(
            module,
            Module {
                type_section: Some(vec![FuncType::default()]),
                function_section: Some(vec![0]),
                code_section: Some(vec![Function {
                    locals: vec![],
                    code: vec![Instruction::End],
                }]),
                ..Default::default()
            }
        );
        Ok(())
    }
    #[test]
    fn decode_func_param() -> Result<()> {
        let wasm = wat::parse_str("(module (func (param i32 i64)))")?;
        let module = Module::new(&wasm)?;
        assert_eq!(
            module,
            Module {
                type_section: Some(vec![FuncType {
                    params: vec![ValueType::I32, ValueType::I64],
                    results: vec![],
                }]),
                function_section: Some(vec![0]),
                code_section: Some(vec![Function {
                    locals: vec![],
                    code: vec![Instruction::End],
                }]),
                ..Default::default()
            }
        );
        Ok(())
    }

    #[test]
    fn decode_func_local() -> Result<()> {
        let wasm = wat::parse_file("src/fixtures/func_local.wat")?;
        let module = Module::new(&wasm)?;
        assert_eq!(
            module,
            Module {
                type_section: Some(vec![FuncType::default()]),
                function_section: Some(vec![0]),
                code_section: Some(vec![Function {
                    locals: vec![
                        FunctionLocal {
                            type_count: 1,
                            value_type: ValueType::I32,
                        },
                        FunctionLocal {
                            type_count: 2,
                            value_type: ValueType::I64,
                        },
                    ],
                    code: vec![Instruction::End],
                }]),
                ..Default::default()
            }
        );
        Ok(())
    }

    #[test]
    fn decode_func_add() -> Result<()> {
        let wasm = wat::parse_file("src/fixtures/func_add.wat")?;
        let module = Module::new(&wasm)?;
        assert_eq!(
            module,
            Module {
                type_section: Some(vec![FuncType {
                    params: vec![ValueType::I32, ValueType::I32],
                    results: vec![ValueType::I32],
                }]),
                function_section: Some(vec![0]),
                code_section: Some(vec![Function {
                    locals: vec![],
                    code: vec![
                        Instruction::LocalGet(0),
                        Instruction::LocalGet(1),
                        Instruction::I32Add,
                        Instruction::End
                    ],
                }]),
                ..Default::default()
            }
        );
        Ok(())
    }
    #[test]

    fn decode_export_func_add() -> Result<()> {
        let wasm = wat::parse_file("src/fixtures/func_export_start_i64add.wat")?;
        let module = Module::new(&wasm)?;
        assert_eq!(
            module,
            Module {
                type_section: Some(vec![FuncType {
                    params: vec![ValueType::I64, ValueType::I64],
                    results: vec![ValueType::I64],
                }]),
                function_section: Some(vec![0]),
                code_section: Some(vec![Function {
                    locals: vec![],
                    code: vec![
                        Instruction::LocalGet(0),
                        Instruction::LocalGet(1),
                        Instruction::I64Add,
                        Instruction::End
                    ],
                }]),
                export_section: Some(vec![Export {
                    name: "_start".to_string(),
                    desc: ExportDesc::Func(0),
                }]),
                ..Default::default()
            }
        );
        Ok(())
    }

    #[test]
    fn decode_i64_const() -> Result<()> {
        let wasm = wat::parse_file("src/fixtures/i64_const.wat")?;
        let module = Module::new(&wasm)?;
        assert_eq!(
            module,
            Module {
                type_section: Some(vec![FuncType {
                    params: vec![],
                    results: vec![],
                }]),
                function_section: Some(vec![0]),
                code_section: Some(vec![Function {
                    locals: vec![],
                    code: vec![Instruction::I64Const(42), Instruction::End],
                }]),
                ..Default::default()
            }
        );
        Ok(())
    }
}
