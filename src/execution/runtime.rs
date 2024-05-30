use super::{
    store::{FuncInst, InternalFuncInst, Store},
    value::Value,
};
use crate::binary::{
    instruction::Instruction,
    module::Module,
    types::{ExportDesc, ValueType},
};
use anyhow::{anyhow, bail, Result};

#[derive(Default)]
pub struct Frame {
    pub pc: isize,               // プログラムカウンタ
    pub sp: usize,               // スタックポインタ
    pub insts: Vec<Instruction>, // 命令列
    pub arity: usize,            // 戻り値の個数
    pub locals: Vec<Value>,      // ローカル変数
}

#[derive(Default)]
pub struct Runtime {
    pub store: Store,
    pub stack: Vec<Value>,
    pub call_stack: Vec<Frame>,
}

impl Runtime {
    pub fn instantiate(wasm: impl AsRef<[u8]>) -> Result<Self> {
        let module = Module::new(wasm.as_ref())?;
        let store = Store::new(module)?;
        Ok(Self {
            store,
            ..Default::default()
        })
    }

    fn execute(&mut self) -> Result<()> {
        loop {
            let Some(frame) = self.call_stack.last_mut() else {
                break;
            };

            frame.pc += 1;
            let Some(inst) = frame.insts.get(frame.pc as usize) else {
                break;
            };

            match inst {
                Instruction::End => {
                    // コールスタックからフレームをpopし、
                    // フレームの情報からspとarityを取り出し、スタックを戻す
                    let Some(frame) = self.call_stack.pop() else {
                        bail!("not found frame");
                    };
                    let Frame { sp, arity, .. } = frame;
                    stack_unwind(&mut self.stack, sp, arity)?;
                }
                Instruction::LocalGet(idx) => {
                    let Some(value) = frame.locals.get(*idx as usize) else {
                        bail!("not found local");
                    };
                    self.stack.push(*value);
                }
                Instruction::I64Const(val) => self.stack.push(Value::I64(*val)),
                Instruction::I32Add => {
                    let (Some(rhs), Some(lhs)) = (self.stack.pop(), self.stack.pop()) else {
                        bail!("not found any value in the stack");
                    };
                    let result = lhs + rhs;
                    self.stack.push(result);
                }
                Instruction::I64Add => {
                    let (Some(rhs), Some(lhs)) = (self.stack.pop(), self.stack.pop()) else {
                        bail!("not found any value in the stack");
                    };
                    let result = lhs + rhs;
                    self.stack.push(result);
                }
            }
        }
        Ok(())
    }

    pub fn call(&mut self, name: impl Into<String>, args: Vec<Value>) -> Result<Option<Value>> {
        let idx = match self
            .store
            .module
            .exports
            .get(&name.into())
            .ok_or(anyhow!("not found export function"))?
            .desc
        {
            ExportDesc::Func(idx) => idx as usize,
        };
        let Some(func_inst) = self.store.funcs.get(idx) else {
            bail!("not found func")
        };
        for arg in args {
            self.stack.push(arg);
        }
        match func_inst {
            FuncInst::Internal(func) => self.invoke_internal(func.clone()),
        }
    }

    fn invoke_internal(&mut self, func: InternalFuncInst) -> Result<Option<Value>> {
        // 関数の引数の個数
        let bottom = self.stack.len() - func.func_type.params.len();

        // 引数の数、スタックから値をpop
        let mut locals = self.stack.split_off(bottom);

        // ローカル変数の初期化
        for local in func.code.locals.iter() {
            match local {
                ValueType::I32 => locals.push(Value::I32(0)),
                ValueType::I64 => locals.push(Value::I64(0)),
            }
        }

        // 戻り値の個数
        let arity = func.func_type.results.len();

        let frame = Frame {
            pc: -1,
            sp: self.stack.len(),
            insts: func.code.body.clone(),
            arity,
            locals,
        };

        // コールスタックにフレームをpush
        self.call_stack.push(frame);

        // 実行
        if let Err(e) = self.execute() {
            self.cleanup();
            bail!("failed to execute instructions: {}", e)
        }

        if arity > 0 {
            let Some(value) = self.stack.pop() else {
                bail!("not found return value")
            };
            return Ok(Some(value));
        }
        Ok(None)
    }

    fn cleanup(&mut self) {
        self.stack = vec![];
        self.call_stack = vec![];
    }
}

pub fn stack_unwind(stack: &mut Vec<Value>, sp: usize, arity: usize) -> Result<()> {
    if arity > 0 {
        let Some(value) = stack.pop() else {
            bail!("not found return value");
        };
        stack.drain(sp..);
        stack.push(value);
    } else {
        stack.drain(sp..);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Runtime;
    use crate::execution::value::Value;
    use anyhow::Result;

    #[test]
    fn execute_export_start_i64add() -> Result<()> {
        let wasm = wat::parse_file("src/fixtures/func_export_start_i64add.wat")?;
        let mut runtime = Runtime::instantiate(wasm)?;
        let tests = vec![(2, 3, 5)];

        for (lhs, rhs, want) in tests {
            let args = vec![Value::I64(lhs), Value::I64(rhs)];
            let result = runtime.call("_start", args)?;
            assert_eq!(result, Some(Value::I64(want)))
        }
        Ok(())
    }
}
