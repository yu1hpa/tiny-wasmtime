use crate::binary::{
    instruction::Instruction,
    module::Module,
    types::{ExportDesc, FuncType, ValueType},
};
use anyhow::{bail, Result};
use std::collections::HashMap;

#[derive(Clone)]
pub struct Func {
    pub locals: Vec<ValueType>,
    pub body: Vec<Instruction>,
}

#[derive(Clone)]
pub struct InternalFuncInst {
    pub func_type: FuncType,
    pub code: Func,
}

#[derive(Clone)]
pub enum FuncInst {
    Internal(InternalFuncInst),
}

pub struct ExportInst {
    pub name: String,
    pub desc: ExportDesc,
}

#[derive(Default)]
pub struct ModuleInst {
    pub exports: HashMap<String, ExportInst>,
}

#[derive(Default)]
pub struct Store {
    pub funcs: Vec<FuncInst>,
    pub module: ModuleInst,
}

impl Store {
    // Function SectionとCode SectionからStoreに必要な情報を取得
    pub fn new(module: Module) -> Result<Self> {
        let func_type_idxs = match module.function_section {
            Some(ref idxs) => idxs.clone(),
            _ => vec![],
        };

        let mut funcs = vec![];

        if let Some(ref code_section) = module.code_section {
            for (func_body, type_idx) in code_section.iter().zip(func_type_idxs.into_iter()) {
                let Some(ref func_types) = module.type_section else {
                  bail!("not found type_section")
                };

                let Some(func_type) = func_types.get(type_idx as usize) else {
                    bail!("not found func type in type_section")
                };

                let mut locals = Vec::with_capacity(func_body.locals.len());
                for local in func_body.locals.iter() {
                    for _ in 0..local.type_count {
                        locals.push(local.value_type.clone());
                    }
                }

                let func = FuncInst::Internal(InternalFuncInst {
                    func_type: func_type.clone(),
                    code: Func {
                        locals,
                        body: func_body.code.clone(),
                    },
                });
                funcs.push(func)
            }
        }

        let mut exports = HashMap::default();
        if let Some(ref sections) = module.export_section {
            for export in sections {
                let name = export.name.clone();
                let export_inst = ExportInst {
                    name: name.clone(),
                    desc: export.desc.clone(),
                };
                exports.insert(name, export_inst);
            }
        };
        let module_inst = ModuleInst { exports };

        Ok(Self {
            funcs,
            module: module_inst,
        })
    }
}
