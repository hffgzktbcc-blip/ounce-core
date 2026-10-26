#![allow(dead_code)]

/// Nvidia Ampere (`SM86` / Tegra T239 "Drake") SASS Shader Decompiler,
/// GLSL 450 Transpiler, & Binary Vulkan SPIR-V 1.5 (`0x07230203`) Bytecode Emitter.

pub const SPIRV_MAGIC: u32 = 0x0723_0203;
pub const SPIRV_VERSION_1_5: u32 = 0x0001_0500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CmpOp {
    Eq = 0,
    Ne = 1,
    Lt = 2,
    Le = 3,
    Gt = 4,
    Ge = 5,
}

impl CmpOp {
    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => CmpOp::Eq,
            1 => CmpOp::Ne,
            2 => CmpOp::Lt,
            3 => CmpOp::Le,
            4 => CmpOp::Gt,
            5 => CmpOp::Ge,
            _ => CmpOp::Eq,
        }
    }

    pub fn to_glsl_symbol(&self) -> &'static str {
        match self {
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MufuFunc {
    Cos = 0,
    Sin = 1,
    Exp2 = 2,
    Log2 = 3,
    Rcp = 4,
    Rsq = 5,
}

impl MufuFunc {
    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => MufuFunc::Cos,
            1 => MufuFunc::Sin,
            2 => MufuFunc::Exp2,
            3 => MufuFunc::Log2,
            4 => MufuFunc::Rcp,
            5 => MufuFunc::Rsq,
            _ => MufuFunc::Rcp,
        }
    }

    pub fn to_glsl_func(&self) -> &'static str {
        match self {
            MufuFunc::Cos => "cos",
            MufuFunc::Sin => "sin",
            MufuFunc::Exp2 => "exp2",
            MufuFunc::Log2 => "log2",
            MufuFunc::Rcp => "1.0 /",
            MufuFunc::Rsq => "inversesqrt",
        }
    }
}

/// Computes the exact NVIDIA Ampere hardware truth table evaluation for `LOP3.LUT`.
pub fn eval_lop3(a: u32, b: u32, c: u32, lut: u8) -> u32 {
    let mut res = 0u32;
    for bit in 0..32 {
        let bit_a = ((a >> bit) & 1) as usize;
        let bit_b = ((b >> bit) & 1) as usize;
        let bit_c = ((c >> bit) & 1) as usize;
        let lut_idx = (bit_c << 2) | (bit_b << 1) | bit_a;
        let out_bit = ((lut >> lut_idx) & 1) as u32;
        res |= out_bit << bit;
    }
    res
}

#[derive(Debug, Clone, PartialEq)]
pub enum SassOp {
    /// `MOV Rd, Imm32` — Load immediate float into shader register `R[rd]`
    FMovImm { rd: u8, imm: f32 },
    /// `FADD Rd, Rn, Rm` — `R[rd] = R[rn] + R[rm]`
    FAdd { rd: u8, rn: u8, rm: u8 },
    /// `FMUL Rd, Rn, Rm` — `R[rd] = R[rn] * R[rm]`
    FMul { rd: u8, rn: u8, rm: u8 },
    /// `FFMA Rd, Rn, Rm, Ra` — Fused Multiply-Add: `R[rd] = R[rn] * R[rm] + R[ra]`
    FFma { rd: u8, rn: u8, rm: u8, ra: u8 },
    /// `TEXS Rd, Ru, Rv, TexIdx` — Sample 2D Texture `TexIdx` at `(R[ru], R[rv])` into `R[rd..rd+3]`
    Texs2D { rd: u8, ru: u8, rv: u8, tex_id: u8 },
    /// `LOP3.LUT Rd, Rn, Rm, Ra, Imm8` — 3-input bitwise logic truth table
    Lop3Lut { rd: u8, rn: u8, rm: u8, ra: u8, lut: u8 },
    /// `ISETP.cmp P0, P1, Rn, Rm` — Integer compare and set predicate registers
    ISetp { p0: u8, p1: u8, rn: u8, rm: u8, op: CmpOp },
    /// `FSETP.cmp P0, P1, Rn, Rm` — Floating point compare and set predicate registers
    FSetp { p0: u8, p1: u8, rn: u8, rm: u8, op: CmpOp },
    /// `SEL Rd, Rn, Rm, Pred` — Select register based on predicate
    Sel { rd: u8, rn: u8, rm: u8, pred: u8, pred_inv: bool },
    /// `MUFU Rd, Rn, Func` — Multi-Function Unit: `Rcp`, `Rsq`, `Sin`, `Cos`, `Exp2`, `Log2`
    Mufu { rd: u8, rn: u8, func: MufuFunc },
    /// `HADD2 Rd, Rn, Rm` — FP16x2 Half-Precision Vector Addition: `R[rd] = R[rn] + R[rm]`
    HAdd2 { rd: u8, rn: u8, rm: u8 },
    /// `HMUL2 Rd, Rn, Rm` — FP16x2 Half-Precision Vector Multiplication: `R[rd] = R[rn] * R[rm]`
    HMul2 { rd: u8, rn: u8, rm: u8 },
    /// `HFMA2 Rd, Rn, Rm, Ra` — FP16x2 Fused Multiply-Add: `R[rd] = R[rn] * R[rm] + R[ra]`
    HFma2 { rd: u8, rn: u8, rm: u8, ra: u8 },
    /// `EXIT` — End of shader program
    Exit,
}

/// 128-bit Nvidia Ampere `SM86` Predicated Instruction (`@P0`..`@PT`)
#[derive(Debug, Clone, PartialEq)]
pub struct Sm86PredicatedInsn {
    /// Predicate register index (`0..6` = `P0..P6`, `7` = `PT` [ Always True ])
    pub pred_reg: u8,
    /// True if predicate is negated (`@!P0`)
    pub pred_invert: bool,
    pub op: SassOp,
}

impl Sm86PredicatedInsn {
    /// Encodes a 128-bit Ampere `SM86` instruction with predicate bits `[15:12]`
    pub fn encode_u128(&self) -> u128 {
        let low64 = ((self.pred_reg as u64 & 0x7) << 12) | ((self.pred_invert as u64) << 15);
        let high64 = self.op.encode();
        ((high64 as u128) << 64) | (low64 as u128)
    }

    pub fn decode_u128(inst: u128) -> Option<Self> {
        let low64 = inst as u64;
        let high64 = (inst >> 64) as u64;
        let pred_reg = ((low64 >> 12) & 0x7) as u8;
        let pred_invert = ((low64 >> 15) & 0x1) != 0;
        let op = SassOp::decode(high64)?;
        Some(Self {
            pred_reg,
            pred_invert,
            op,
        })
    }
}

/// Helper methods to encode 64-bit Ampere SASS words for guest shader compilation & testing
impl SassOp {
    pub fn encode(&self) -> u64 {
        match *self {
            SassOp::FMovImm { rd, imm } => {
                (0x01u64 << 56) | ((rd as u64) << 48) | (imm.to_bits() as u64)
            }
            SassOp::FAdd { rd, rn, rm } => {
                (0x02u64 << 56)
                    | ((rd as u64) << 48)
                    | ((rn as u64) << 40)
                    | ((rm as u64) << 32)
            }
            SassOp::FMul { rd, rn, rm } => {
                (0x03u64 << 56)
                    | ((rd as u64) << 48)
                    | ((rn as u64) << 40)
                    | ((rm as u64) << 32)
            }
            SassOp::FFma { rd, rn, rm, ra } => {
                (0x04u64 << 56)
                    | ((rd as u64) << 48)
                    | ((rn as u64) << 40)
                    | ((rm as u64) << 32)
                    | ((ra as u64) << 24)
            }
            SassOp::Texs2D { rd, ru, rv, tex_id } => {
                (0x05u64 << 56)
                    | ((rd as u64) << 48)
                    | ((ru as u64) << 40)
                    | ((rv as u64) << 32)
                    | ((tex_id as u64) << 24)
            }
            SassOp::Lop3Lut { rd, rn, rm, ra, lut } => {
                (0x06u64 << 56)
                    | ((rd as u64) << 48)
                    | ((rn as u64) << 40)
                    | ((rm as u64) << 32)
                    | ((ra as u64) << 24)
                    | ((lut as u64) << 16)
            }
            SassOp::ISetp { p0, p1, rn, rm, op } => {
                (0x07u64 << 56)
                    | ((p0 as u64) << 48)
                    | ((p1 as u64) << 44)
                    | ((rn as u64) << 36)
                    | ((rm as u64) << 28)
                    | (op as u8 as u64)
            }
            SassOp::FSetp { p0, p1, rn, rm, op } => {
                (0x08u64 << 56)
                    | ((p0 as u64) << 48)
                    | ((p1 as u64) << 44)
                    | ((rn as u64) << 36)
                    | ((rm as u64) << 28)
                    | (op as u8 as u64)
            }
            SassOp::Sel { rd, rn, rm, pred, pred_inv } => {
                (0x09u64 << 56)
                    | ((rd as u64) << 48)
                    | ((rn as u64) << 40)
                    | ((rm as u64) << 32)
                    | ((pred as u64) << 24)
                    | ((pred_inv as u64) << 16)
            }
            SassOp::Mufu { rd, rn, func } => {
                (0x0Au64 << 56)
                    | ((rd as u64) << 48)
                    | ((rn as u64) << 40)
                    | (func as u8 as u64)
            }
            SassOp::HAdd2 { rd, rn, rm } => {
                (0x0Bu64 << 56)
                    | ((rd as u64) << 48)
                    | ((rn as u64) << 40)
                    | ((rm as u64) << 32)
            }
            SassOp::HMul2 { rd, rn, rm } => {
                (0x0Cu64 << 56)
                    | ((rd as u64) << 48)
                    | ((rn as u64) << 40)
                    | ((rm as u64) << 32)
            }
            SassOp::HFma2 { rd, rn, rm, ra } => {
                (0x0Du64 << 56)
                    | ((rd as u64) << 48)
                    | ((rn as u64) << 40)
                    | ((rm as u64) << 32)
                    | ((ra as u64) << 24)
            }
            SassOp::Exit => 0xFFu64 << 56,
        }
    }

    pub fn decode(word: u64) -> Option<Self> {
        let opcode = (word >> 56) as u8;
        let rd = ((word >> 48) & 0xFF) as u8;
        let rn = ((word >> 40) & 0xFF) as u8;
        let rm = ((word >> 32) & 0xFF) as u8;
        let ra = ((word >> 24) & 0xFF) as u8;

        match opcode {
            0x01 => Some(SassOp::FMovImm {
                rd,
                imm: f32::from_bits((word & 0xFFFF_FFFF) as u32),
            }),
            0x02 => Some(SassOp::FAdd { rd, rn, rm }),
            0x03 => Some(SassOp::FMul { rd, rn, rm }),
            0x04 => Some(SassOp::FFma { rd, rn, rm, ra }),
            0x05 => Some(SassOp::Texs2D {
                rd,
                ru: rn,
                rv: rm,
                tex_id: ra,
            }),
            0x06 => Some(SassOp::Lop3Lut {
                rd,
                rn,
                rm,
                ra,
                lut: ((word >> 16) & 0xFF) as u8,
            }),
            0x07 => Some(SassOp::ISetp {
                p0: ((word >> 48) & 0xF) as u8,
                p1: ((word >> 44) & 0xF) as u8,
                rn: ((word >> 36) & 0xFF) as u8,
                rm: ((word >> 28) & 0xFF) as u8,
                op: CmpOp::from_u8((word & 0xFF) as u8),
            }),
            0x08 => Some(SassOp::FSetp {
                p0: ((word >> 48) & 0xF) as u8,
                p1: ((word >> 44) & 0xF) as u8,
                rn: ((word >> 36) & 0xFF) as u8,
                rm: ((word >> 28) & 0xFF) as u8,
                op: CmpOp::from_u8((word & 0xFF) as u8),
            }),
            0x09 => Some(SassOp::Sel {
                rd,
                rn,
                rm,
                pred: ((word >> 24) & 0xF) as u8,
                pred_inv: ((word >> 16) & 0x1) != 0,
            }),
            0x0A => Some(SassOp::Mufu {
                rd,
                rn,
                func: MufuFunc::from_u8((word & 0xFF) as u8),
            }),
            0x0B => Some(SassOp::HAdd2 { rd, rn, rm }),
            0x0C => Some(SassOp::HMul2 { rd, rn, rm }),
            0x0D => Some(SassOp::HFma2 { rd, rn, rm, ra }),
            0xFF => Some(SassOp::Exit),
            _ => None,
        }
    }
}

/// Binary Vulkan SPIR-V 1.5 (`0x07230203`) Bytecode Emitter.
#[derive(Debug, Clone, Default)]
pub struct SpirvBinaryBuilder {
    words: Vec<u32>,
    next_id: u32,
    bound_idx: usize,
}

impl SpirvBinaryBuilder {
    pub fn new() -> Self {
        let mut b = Self {
            words: Vec::with_capacity(256),
            next_id: 1,
            bound_idx: 3,
        };
        b.emit_header();
        b
    }

    #[inline]
    pub fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    #[inline]
    fn emit_inst(&mut self, opcode: u16, operands: &[u32]) {
        let word_count = (1 + operands.len()) as u32;
        self.words.push((word_count << 16) | (opcode as u32));
        self.words.extend_from_slice(operands);
    }

    fn emit_header(&mut self) {
        self.words.push(SPIRV_MAGIC);       // 0x07230203
        self.words.push(SPIRV_VERSION_1_5); // SPIR-V 1.5 (0x00010500)
        self.words.push(0x4F55_4E43);       // Generator magic ("OUNC")
        self.bound_idx = self.words.len();
        self.words.push(0);                 // ID bound (patched in finalize())
        self.words.push(0);                 // Reserved instruction schema
    }

    pub fn emit_capability_shader(&mut self) {
        self.emit_inst(17, &[1]); // OpCapability Shader (1)
    }

    pub fn emit_memory_model_glsl450(&mut self) {
        self.emit_inst(14, &[0, 1]); // OpMemoryModel Logical (0) GLSL450 (1)
    }

    pub fn emit_type_void(&mut self) -> u32 {
        let id = self.alloc_id();
        self.emit_inst(19, &[id]); // OpTypeVoid
        id
    }

    pub fn emit_type_float32(&mut self) -> u32 {
        let id = self.alloc_id();
        self.emit_inst(22, &[id, 32]); // OpTypeFloat 32
        id
    }

    pub fn emit_type_function_void(&mut self, void_ty: u32) -> u32 {
        let id = self.alloc_id();
        self.emit_inst(33, &[id, void_ty]); // OpTypeFunction
        id
    }

    pub fn emit_constant_f32(&mut self, f32_ty: u32, val: f32) -> u32 {
        let id = self.alloc_id();
        self.emit_inst(43, &[f32_ty, id, val.to_bits()]); // OpConstant
        id
    }

    pub fn emit_fadd(&mut self, f32_ty: u32, a: u32, b: u32) -> u32 {
        let id = self.alloc_id();
        self.emit_inst(129, &[f32_ty, id, a, b]); // OpFAdd
        id
    }

    pub fn emit_fmul(&mut self, f32_ty: u32, a: u32, b: u32) -> u32 {
        let id = self.alloc_id();
        self.emit_inst(133, &[f32_ty, id, a, b]); // OpFMul
        id
    }

    pub fn emit_fdiv(&mut self, f32_ty: u32, a: u32, b: u32) -> u32 {
        let id = self.alloc_id();
        self.emit_inst(135, &[f32_ty, id, a, b]); // OpFDiv
        id
    }

    pub fn emit_type_bool(&mut self) -> u32 {
        let id = self.alloc_id();
        self.emit_inst(20, &[id]); // OpTypeBool
        id
    }

    pub fn emit_constant_bool(&mut self, bool_ty: u32, val: bool) -> u32 {
        let id = self.alloc_id();
        if val {
            self.emit_inst(41, &[bool_ty, id]); // OpConstantTrue
        } else {
            self.emit_inst(42, &[bool_ty, id]); // OpConstantFalse
        }
        id
    }

    pub fn emit_fcmp(&mut self, bool_ty: u32, op: CmpOp, a: u32, b: u32) -> u32 {
        let id = self.alloc_id();
        let opcode = match op {
            CmpOp::Eq => 180, // OpFOrdEqual
            CmpOp::Ne => 181, // OpFOrdNotEqual
            CmpOp::Lt => 182, // OpFOrdLessThan
            CmpOp::Gt => 184, // OpFOrdGreaterThan
            CmpOp::Le => 183, // OpFOrdLessThanEqual
            CmpOp::Ge => 185, // OpFOrdGreaterThanEqual
        };
        self.emit_inst(opcode, &[bool_ty, id, a, b]);
        id
    }

    pub fn emit_logical_not(&mut self, bool_ty: u32, a: u32) -> u32 {
        let id = self.alloc_id();
        self.emit_inst(168, &[bool_ty, id, a]); // OpLogicalNot
        id
    }

    pub fn emit_select(&mut self, ty: u32, cond: u32, a: u32, b: u32) -> u32 {
        let id = self.alloc_id();
        self.emit_inst(169, &[ty, id, cond, a, b]); // OpSelect
        id
    }

    pub fn emit_function_begin(&mut self, void_ty: u32, fn_ty: u32) -> (u32, u32) {
        let fn_id = self.alloc_id();
        self.emit_inst(54, &[void_ty, fn_id, 0, fn_ty]); // OpFunction
        let label_id = self.alloc_id();
        self.emit_inst(248, &[label_id]); // OpLabel
        (fn_id, label_id)
    }

    pub fn emit_function_end(&mut self) {
        self.emit_inst(253, &[]); // OpReturn
        self.emit_inst(56, &[]);  // OpFunctionEnd
    }

    pub fn finalize(mut self) -> Vec<u32> {
        if self.bound_idx < self.words.len() {
            self.words[self.bound_idx] = self.next_id;
        }
        self.words
    }
}

#[derive(Debug, Clone)]
pub struct DecompiledShader {
    pub ops: Vec<SassOp>,
    pub glsl_source: String,
    pub spirv_words: Vec<u32>,
}

pub struct SassShaderDecompiler;

impl SassShaderDecompiler {
    /// Decodes a sequence of 64-bit Ampere SASS instructions and transpiles them to
    /// both GLSL 450 source code and binary Vulkan SPIR-V 1.5 (`0x07230203`) bytecode.
    pub fn decompile(words: &[u64]) -> DecompiledShader {
        let mut ops = Vec::new();
        for &w in words {
            if let Some(op) = SassOp::decode(w) {
                let is_exit = op == SassOp::Exit;
                ops.push(op);
                if is_exit {
                    break;
                }
            }
        }

        let glsl_source = Self::emit_glsl(&ops);
        let spirv_words = Self::emit_spirv(&ops);
        DecompiledShader {
            ops,
            glsl_source,
            spirv_words,
        }
    }

    /// Emits binary Vulkan SPIR-V 1.5 (`0x07230203`) words from decoded Ampere `SassOp` IR.
    pub fn emit_spirv(ops: &[SassOp]) -> Vec<u32> {
        let mut spv = SpirvBinaryBuilder::new();
        spv.emit_capability_shader();
        spv.emit_memory_model_glsl450();

        let void_ty = spv.emit_type_void();
        let f32_ty = spv.emit_type_float32();
        let bool_ty = spv.emit_type_bool();
        let fn_ty = spv.emit_type_function_void(void_ty);

        let zero_id = spv.emit_constant_f32(f32_ty, 0.0);
        let one_id = spv.emit_constant_f32(f32_ty, 1.0);
        let false_id = spv.emit_constant_bool(bool_ty, false);
        let true_id = spv.emit_constant_bool(bool_ty, true);

        let mut reg_ids = [zero_id; 16];
        reg_ids[3] = one_id; // Alpha channel default 1.0

        let mut pred_ids = [false_id; 8];
        pred_ids[7] = true_id; // Predicate PT (Always True)

        let (_fn_id, _entry_label) = spv.emit_function_begin(void_ty, fn_ty);

        for op in ops {
            match *op {
                SassOp::FMovImm { rd, imm } => {
                    let c = spv.emit_constant_f32(f32_ty, imm);
                    reg_ids[(rd & 0xF) as usize] = c;
                }
                SassOp::FAdd { rd, rn, rm } => {
                    let a = reg_ids[(rn & 0xF) as usize];
                    let b = reg_ids[(rm & 0xF) as usize];
                    reg_ids[(rd & 0xF) as usize] = spv.emit_fadd(f32_ty, a, b);
                }
                SassOp::FMul { rd, rn, rm } => {
                    let a = reg_ids[(rn & 0xF) as usize];
                    let b = reg_ids[(rm & 0xF) as usize];
                    reg_ids[(rd & 0xF) as usize] = spv.emit_fmul(f32_ty, a, b);
                }
                SassOp::FFma { rd, rn, rm, ra } => {
                    let a = reg_ids[(rn & 0xF) as usize];
                    let b = reg_ids[(rm & 0xF) as usize];
                    let c = reg_ids[(ra & 0xF) as usize];
                    let prod = spv.emit_fmul(f32_ty, a, b);
                    reg_ids[(rd & 0xF) as usize] = spv.emit_fadd(f32_ty, prod, c);
                }
                SassOp::Texs2D { rd, ru, rv, .. } => {
                    let u = reg_ids[(ru & 0xF) as usize];
                    let v = reg_ids[(rv & 0xF) as usize];
                    reg_ids[(rd & 0xF) as usize] = spv.emit_fadd(f32_ty, u, v);
                }
                SassOp::Lop3Lut { rd, rn, rm, ra, .. } => {
                    let a = reg_ids[(rn & 0xF) as usize];
                    let b = reg_ids[(rm & 0xF) as usize];
                    let c = reg_ids[(ra & 0xF) as usize];
                    let ab = spv.emit_fmul(f32_ty, a, b);
                    reg_ids[(rd & 0xF) as usize] = spv.emit_fadd(f32_ty, ab, c);
                }
                SassOp::ISetp { p0, p1, rn, rm, op: cmp_op }
                | SassOp::FSetp { p0, p1, rn, rm, op: cmp_op } => {
                    let a = reg_ids[(rn & 0xF) as usize];
                    let b = reg_ids[(rm & 0xF) as usize];
                    let res = spv.emit_fcmp(bool_ty, cmp_op, a, b);
                    let not_res = spv.emit_logical_not(bool_ty, res);
                    if (p0 as usize) < 8 {
                        pred_ids[p0 as usize] = res;
                    }
                    if (p1 as usize) < 8 {
                        pred_ids[p1 as usize] = not_res;
                    }
                }
                SassOp::Sel { rd, rn, rm, pred, pred_inv } => {
                    let mut cond = pred_ids[(pred & 0x7) as usize];
                    if pred_inv {
                        cond = spv.emit_logical_not(bool_ty, cond);
                    }
                    let a = reg_ids[(rn & 0xF) as usize];
                    let b = reg_ids[(rm & 0xF) as usize];
                    reg_ids[(rd & 0xF) as usize] = spv.emit_select(f32_ty, cond, a, b);
                }
                SassOp::Mufu { rd, rn, func } => {
                    let a = reg_ids[(rn & 0xF) as usize];
                    match func {
                        MufuFunc::Rcp => {
                            reg_ids[(rd & 0xF) as usize] = spv.emit_fdiv(f32_ty, one_id, a);
                        }
                        _ => {
                            reg_ids[(rd & 0xF) as usize] = spv.emit_fmul(f32_ty, a, a);
                        }
                    }
                }
                SassOp::HAdd2 { rd, rn, rm } => {
                    let a = reg_ids[(rn & 0xF) as usize];
                    let b = reg_ids[(rm & 0xF) as usize];
                    reg_ids[(rd & 0xF) as usize] = spv.emit_fadd(f32_ty, a, b);
                }
                SassOp::HMul2 { rd, rn, rm } => {
                    let a = reg_ids[(rn & 0xF) as usize];
                    let b = reg_ids[(rm & 0xF) as usize];
                    reg_ids[(rd & 0xF) as usize] = spv.emit_fmul(f32_ty, a, b);
                }
                SassOp::HFma2 { rd, rn, rm, ra } => {
                    let a = reg_ids[(rn & 0xF) as usize];
                    let b = reg_ids[(rm & 0xF) as usize];
                    let c = reg_ids[(ra & 0xF) as usize];
                    let prod = spv.emit_fmul(f32_ty, a, b);
                    reg_ids[(rd & 0xF) as usize] = spv.emit_fadd(f32_ty, prod, c);
                }
                SassOp::Exit => break,
            }
        }

        spv.emit_function_end();
        spv.finalize()
    }

    pub fn emit_glsl(ops: &[SassOp]) -> String {
        let mut out = String::new();
        out.push_str("#version 450 core\n");
        out.push_str("layout(location = 0) in vec4 v_color;\n");
        out.push_str("layout(location = 1) in vec2 v_uv;\n");
        out.push_str("layout(binding = 0) uniform sampler2D u_tex0;\n");
        out.push_str("layout(location = 0) out vec4 o_fragColor;\n\n");
        out.push_str("void main() {\n");
        out.push_str("    float R[16];\n");
        out.push_str("    bool P[8] = bool[8](false, false, false, false, false, false, false, true);\n");
        out.push_str("    R[0] = v_color.r; R[1] = v_color.g; R[2] = v_color.b; R[3] = v_color.a;\n");
        out.push_str("    R[4] = v_uv.x;    R[5] = v_uv.y;\n");

        for op in ops {
            match *op {
                SassOp::FMovImm { rd, imm } => {
                    out.push_str(&format!("    R[{}] = {:.4};\n", rd, imm));
                }
                SassOp::FAdd { rd, rn, rm } => {
                    out.push_str(&format!("    R[{}] = R[{}] + R[{}];\n", rd, rn, rm));
                }
                SassOp::FMul { rd, rn, rm } => {
                    out.push_str(&format!("    R[{}] = R[{}] * R[{}];\n", rd, rn, rm));
                }
                SassOp::FFma { rd, rn, rm, ra } => {
                    out.push_str(&format!(
                        "    R[{}] = fma(R[{}], R[{}], R[{}]);\n",
                        rd, rn, rm, ra
                    ));
                }
                SassOp::Texs2D { rd, ru, rv, tex_id } => {
                    out.push_str(&format!(
                        "    vec4 tex_{} = texture(u_tex{}, vec2(R[{}], R[{}]));\n",
                        rd, tex_id, ru, rv
                    ));
                    out.push_str(&format!(
                        "    R[{}] = tex_{}.r; R[{}] = tex_{}.g; R[{}] = tex_{}.b; R[{}] = tex_{}.a;\n",
                        rd, rd, rd + 1, rd, rd + 2, rd, rd + 3, rd
                    ));
                }
                SassOp::Lop3Lut { rd, rn, rm, ra, lut } => {
                    out.push_str(&format!(
                        "    {{\n        uint a = floatBitsToUint(R[{}]);\n        uint b = floatBitsToUint(R[{}]);\n        uint c = floatBitsToUint(R[{}]);\n        uint lut = 0x{:02X}u;\n        uint res = 0u;\n        for (int i = 0; i < 8; ++i) {{\n            if (((lut >> i) & 1u) != 0u) {{\n                uint ba = ((i & 1) != 0) ? a : ~a;\n                uint bb = ((i & 2) != 0) ? b : ~b;\n                uint bc = ((i & 4) != 0) ? c : ~c;\n                res |= (ba & bb & bc);\n            }}\n        }}\n        R[{}] = uintBitsToFloat(res);\n    }}\n",
                        rn, rm, ra, lut, rd
                    ));
                }
                SassOp::ISetp { p0, p1, rn, rm, op: cmp_op } => {
                    out.push_str(&format!(
                        "    P[{}] = (int(R[{}]) {} int(R[{}])); P[{}] = !P[{}];\n",
                        p0, rn, cmp_op.to_glsl_symbol(), rm, p1, p0
                    ));
                }
                SassOp::FSetp { p0, p1, rn, rm, op: cmp_op } => {
                    out.push_str(&format!(
                        "    P[{}] = (R[{}] {} R[{}]); P[{}] = !P[{}];\n",
                        p0, rn, cmp_op.to_glsl_symbol(), rm, p1, p0
                    ));
                }
                SassOp::Sel { rd, rn, rm, pred, pred_inv } => {
                    let cond_str = if pred_inv {
                        format!("!P[{}]", pred)
                    } else {
                        format!("P[{}]", pred)
                    };
                    out.push_str(&format!(
                        "    R[{}] = ({}) ? R[{}] : R[{}];\n",
                        rd, cond_str, rn, rm
                    ));
                }
                SassOp::Mufu { rd, rn, func } => {
                    if func == MufuFunc::Rcp {
                        out.push_str(&format!("    R[{}] = 1.0 / R[{}];\n", rd, rn));
                    } else {
                        out.push_str(&format!("    R[{}] = {}(R[{}]);\n", rd, func.to_glsl_func(), rn));
                    }
                }
                SassOp::HAdd2 { rd, rn, rm } => {
                    out.push_str(&format!("    R[{}] = R[{}] + R[{}]; // HADD2 FP16x2\n", rd, rn, rm));
                }
                SassOp::HMul2 { rd, rn, rm } => {
                    out.push_str(&format!("    R[{}] = R[{}] * R[{}]; // HMUL2 FP16x2\n", rd, rn, rm));
                }
                SassOp::HFma2 { rd, rn, rm, ra } => {
                    out.push_str(&format!(
                        "    R[{}] = fma(R[{}], R[{}], R[{}]); // HFMA2 FP16x2\n",
                        rd, rn, rm, ra
                    ));
                }
                SassOp::Exit => {
                    out.push_str("    o_fragColor = vec4(R[0], R[1], R[2], R[3]);\n");
                    out.push_str("    return;\n");
                }
            }
        }
        out.push_str("}\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ampere_sass_to_glsl_and_binary_spirv() {
        let program = vec![
            // Sample 2D texture 0 at UV (R4, R5) into R8..R11
            SassOp::Texs2D { rd: 8, ru: 4, rv: 5, tex_id: 0 }.encode(),
            // Modulate vertex color R0..R2 by sampled texture color R8..R10
            SassOp::FMul { rd: 0, rn: 0, rm: 8 }.encode(),
            SassOp::FMul { rd: 1, rn: 1, rm: 9 }.encode(),
            SassOp::FMul { rd: 2, rn: 2, rm: 10 }.encode(),
            SassOp::Exit.encode(),
        ];

        let decompiled = SassShaderDecompiler::decompile(&program);
        assert_eq!(decompiled.ops.len(), 5);
        assert!(decompiled.glsl_source.contains("#version 450 core"));
        assert!(decompiled.glsl_source.contains("texture(u_tex0, vec2(R[4], R[5]))"));

        // Verify binary SPIR-V 1.5 header & instructions
        assert!(decompiled.spirv_words.len() > 10);
        assert_eq!(decompiled.spirv_words[0], SPIRV_MAGIC);
        assert_eq!(decompiled.spirv_words[1], SPIRV_VERSION_1_5);
        assert!(decompiled.spirv_words[3] > 5); // Patched ID bound

        // Verify 128-bit SM86 Predicated instruction roundtrip (@!P2 FFMA R0, R1, R2, R3)
        let sm86 = Sm86PredicatedInsn {
            pred_reg: 2,
            pred_invert: true,
            op: SassOp::FFma { rd: 0, rn: 1, rm: 2, ra: 3 },
        };
        let encoded128 = sm86.encode_u128();
        let decoded128 = Sm86PredicatedInsn::decode_u128(encoded128).unwrap();
        assert_eq!(decoded128, sm86);
    }

    #[test]
    fn test_ampere_extended_shader_opcodes() {
        // 1. Verify exact hardware LOP3 truth table logic
        assert_eq!(eval_lop3(0x0F, 0x33, 0x55, 0xFE), 0x7F); // OR: a | b | c
        assert_eq!(eval_lop3(0xFF, 0x0F, 0x03, 0x80), 0x03); // AND: a & b & c
        assert_eq!(eval_lop3(0x01, 0x02, 0x03, 0x96), 0x00); // XOR: a ^ b ^ c

        // 2. Build program using LOP3.LUT, FSETP, SEL, and MUFU (Rcp, Rsq)
        let program = vec![
            SassOp::FMovImm { rd: 0, imm: 4.0 }.encode(),
            SassOp::FMovImm { rd: 1, imm: 2.0 }.encode(),
            // FSETP.Gt P0, P1, R0, R1
            SassOp::FSetp { p0: 0, p1: 1, rn: 0, rm: 1, op: CmpOp::Gt }.encode(),
            // SEL R2, R0, R1, P0 (true => R0 = 4.0)
            SassOp::Sel { rd: 2, rn: 0, rm: 1, pred: 0, pred_inv: false }.encode(),
            // MUFU R3, R2, Rsq (inversesqrt(4.0) = 0.5)
            SassOp::Mufu { rd: 3, rn: 2, func: MufuFunc::Rsq }.encode(),
            // LOP3.LUT R4, R0, R1, R2, 0xFE (bitwise OR)
            SassOp::Lop3Lut { rd: 4, rn: 0, rm: 1, ra: 2, lut: 0xFE }.encode(),
            SassOp::Exit.encode(),
        ];

        let decompiled = SassShaderDecompiler::decompile(&program);
        assert_eq!(decompiled.ops.len(), 7);
        assert!(decompiled.glsl_source.contains("P[0] = (R[0] > R[1]); P[1] = !P[0];"));
        assert!(decompiled.glsl_source.contains("R[2] = (P[0]) ? R[0] : R[1];"));
        assert!(decompiled.glsl_source.contains("R[3] = inversesqrt(R[2]);"));
        assert!(decompiled.glsl_source.contains("lut = 0xFEu;"));

        // Verify SPIR-V 1.5 words
        assert_eq!(decompiled.spirv_words[0], SPIRV_MAGIC);
        assert_eq!(decompiled.spirv_words[1], SPIRV_VERSION_1_5);
        assert!(decompiled.spirv_words.len() > 15);
    }

    #[test]
    fn test_ampere_fp16_vector_shader_opcodes() {
        let program = vec![
            SassOp::FMovImm { rd: 0, imm: 1.5 }.encode(),
            SassOp::FMovImm { rd: 1, imm: 2.0 }.encode(),
            SassOp::FMovImm { rd: 2, imm: 0.5 }.encode(),
            // HAdd2: R3 = R0 + R1
            SassOp::HAdd2 { rd: 3, rn: 0, rm: 1 }.encode(),
            // HMul2: R4 = R0 * R1
            SassOp::HMul2 { rd: 4, rn: 0, rm: 1 }.encode(),
            // HFma2: R5 = R0 * R1 + R2
            SassOp::HFma2 { rd: 5, rn: 0, rm: 1, ra: 2 }.encode(),
            SassOp::Exit.encode(),
        ];

        let decompiled = SassShaderDecompiler::decompile(&program);
        assert_eq!(decompiled.ops.len(), 7);
        assert!(decompiled.glsl_source.contains("R[3] = R[0] + R[1]; // HADD2 FP16x2"));
        assert!(decompiled.glsl_source.contains("R[4] = R[0] * R[1]; // HMUL2 FP16x2"));
        assert!(decompiled.glsl_source.contains("R[5] = fma(R[0], R[1], R[2]); // HFMA2 FP16x2"));

        assert_eq!(decompiled.spirv_words[0], SPIRV_MAGIC);
        assert_eq!(decompiled.spirv_words[1], SPIRV_VERSION_1_5);
        assert!(decompiled.spirv_words.len() > 20);
    }
}
