#![forbid(unsafe_code)]
//! The same CPU and memory source as the native runtime, with a bounded learning API.
//! No desktop, GPU, audio, filesystem or OS-service adapter is linked here.
#[path = "../../../src/cpu/mod.rs"]
mod cpu;
#[path = "../../../src/mmu/mod.rs"]
mod mmu;

use cpu::{Cpu, StepResult};
use mmu::{Memory, Permission};
use wasm_bindgen::prelude::*;

const CODE: u64 = 0x1000;
const STACK: u64 = 0x8000;
const PAGE: usize = 4096;
const MAX_WORDS: usize = 256;
const MAX_STEPS: u32 = 10_000;

#[wasm_bindgen]
pub struct InterpreterLab {
    cpu: Cpu,
    memory: Memory,
    words: Vec<u32>,
    initial_v: [u128; 32],
    steps: u32,
    stopped: Option<String>,
    last_word: Option<u32>,
}

impl InterpreterLab {
    fn machine(words: Vec<u32>) -> Self {
        let mut memory = Memory::new();
        memory.map_range(CODE, PAGE, Permission::READ_WRITE_EXECUTE);
        memory.map_range(STACK, PAGE, Permission::READ_WRITE);
        for (i, word) in words.iter().enumerate() {
            memory.write_u32(CODE + i as u64 * 4, *word).expect("bounded code fits mapped page");
        }
        let mut cpu = Cpu::new();
        cpu.regs.pc = CODE;
        cpu.regs.sp = STACK + PAGE as u64;
        let stopped = if words.is_empty() { Some("Load a program to begin".into()) } else { None };
        Self { cpu, memory, words, initial_v: [0; 32], steps: 0, stopped, last_word: None }
    }

    fn advance(&mut self) {
        if self.stopped.is_some() { return; }
        let pc = self.cpu.regs.pc;
        if pc < CODE || pc >= CODE + self.words.len() as u64 * 4 || pc % 4 != 0 {
            self.stopped = Some(format!("PC {pc:#x} is outside the loaded instructions"));
            return;
        }
        self.last_word = self.memory.read_u32(pc).ok();
        self.steps += 1;
        self.stopped = match self.cpu.step(&mut self.memory) {
            Ok(StepResult::Continue) if self.steps >= MAX_STEPS => Some(format!("Step budget reached ({MAX_STEPS})")),
            Ok(StepResult::Continue) => None,
            Ok(StepResult::Svc(id)) => Some(format!("SVC #{id}: stopped; this lab provides no OS services")),
            Ok(StepResult::Halt(reason)) => Some(reason),
            Err(error) => Some(error.to_string()),
        };
    }
}

impl Default for InterpreterLab {
    fn default() -> Self { Self::new() }
}

#[wasm_bindgen]
impl InterpreterLab {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self { Self::machine(Vec::new()) }

    /// Loading is transactional: invalid input preserves the previous machine.
    pub fn load(&mut self, program: &str) -> Result<(), String> {
        if program.len() > 8192 { return Err("Program text exceeds 8 KiB".into()); }
        let words = program.split_whitespace().enumerate().map(|(i, token)| {
            u32::from_str_radix(token.strip_prefix("0x").unwrap_or(token), 16)
                .map_err(|_| format!("Word {} is not a 32-bit hexadecimal instruction", i + 1))
        }).collect::<Result<Vec<_>, _>>()?;
        if words.is_empty() || words.len() > MAX_WORDS {
            return Err(format!("Load between 1 and {MAX_WORDS} instruction words"));
        }
        *self = Self::machine(words);
        Ok(())
    }

    pub fn set_vector(&mut self, index: u32, value: &str) -> Result<(), String> {
        if index >= 32 { return Err("Vector register index must be 0–31".into()); }
        if self.steps != 0 || self.stopped.is_some() { return Err("Set input registers before the first step".into()); }
        let value = u128::from_str_radix(value.strip_prefix("0x").unwrap_or(value), 16)
            .map_err(|_| "Vector input must be a 128-bit hexadecimal value".to_string())?;
        self.initial_v[index as usize] = value;
        self.cpu.regs.v[index as usize] = value;
        Ok(())
    }

    pub fn reset(&mut self) {
        let initial = self.initial_v;
        *self = Self::machine(self.words.clone());
        self.initial_v = initial;
        self.cpu.regs.v = initial;
    }

    pub fn step(&mut self) -> String {
        self.advance();
        self.snapshot()
    }

    pub fn run(&mut self, limit: u32) -> Result<String, String> {
        if limit == 0 || limit > 1000 { return Err("Run between 1 and 1000 steps at a time".into()); }
        for _ in 0..limit {
            if self.stopped.is_some() { break; }
            self.advance();
        }
        Ok(self.snapshot())
    }

    pub fn snapshot(&self) -> String {
        let code = self.words.iter().enumerate().map(|(i, _)| {
            let address = CODE + i as u64 * 4;
            let word = self.memory.read_u32(address).expect("loaded code remains mapped");
            serde_json::json!({"address": format!("{address:#018x}"), "word": format!("{word:08x}"),
                               "decoded": format!("{:?}", cpu::decoder::decode(word))})
        }).collect::<Vec<_>>();
        serde_json::json!({
            "engine": "ounce-core interpreter / WebAssembly",
            "pc": format!("{:#018x}", self.cpu.regs.pc),
            "sp": format!("{:#018x}", self.cpu.regs.sp),
            "x": self.cpu.regs.x.iter().map(|v| format!("{v:#018x}")).collect::<Vec<_>>(),
            "v": self.cpu.regs.v.iter().map(|v| format!("{v:#034x}")).collect::<Vec<_>>(),
            "nzcv": self.cpu.regs.pstate.to_bits() >> 28,
            "fpsr": format!("{:#x}", self.cpu.regs.fpsr),
            "steps": self.steps, "stopped": self.stopped,
            "last_word": self.last_word.map(|w| format!("{w:08x}")),
            "code": code
        }).to_string()
    }
}

#[cfg(test)]
mod lab_tests {
    use super::*;
    fn state(lab: &InterpreterLab) -> serde_json::Value { serde_json::from_str(&lab.snapshot()).unwrap() }

    #[test]
    fn actual_mov_add_and_svc_update_the_machine() {
        let mut lab = InterpreterLab::new();
        lab.load("d2800020 91000800 d4000001").unwrap();
        lab.step(); assert_eq!(state(&lab)["x"][0], "0x0000000000000001");
        lab.step(); assert_eq!(state(&lab)["x"][0], "0x0000000000000003");
        lab.step(); assert!(state(&lab)["stopped"].as_str().unwrap().contains("SVC #0"));
        let stopped = lab.snapshot(); lab.step(); assert_eq!(lab.snapshot(), stopped);
    }

    #[test]
    fn invalid_load_is_transactional_and_faults_are_visible() {
        let mut lab = InterpreterLab::new(); lab.load("d2800020").unwrap();
        let before = lab.snapshot(); assert!(lab.load("not-hex").is_err()); assert_eq!(lab.snapshot(), before);
        assert!(lab.load("").is_err()); assert!(lab.load(&"d503201f ".repeat(257)).is_err());
        lab.load("00000000").unwrap(); lab.step(); assert!(state(&lab)["stopped"].is_string());
    }

    #[test]
    fn fixed_vector_case_and_reset_preserve_inputs() {
        let mut lab = InterpreterLab::new(); lab.load("6e62dc20 d4000001").unwrap();
        lab.set_vector(2, "7ff00000000000007ff0000000000000").unwrap(); lab.step();
        assert_eq!(state(&lab)["v"][0], "0x7ff80000000000007ff8000000000000");
        assert!(lab.set_vector(2, "0").is_err()); lab.reset();
        assert_eq!(state(&lab)["v"][2], "0x7ff00000000000007ff0000000000000");
        assert_eq!(state(&lab)["steps"], 0);
    }

    #[test]
    fn runaway_code_is_bounded_and_bad_run_limits_are_rejected() {
        let mut lab = InterpreterLab::new(); lab.load("14000000").unwrap();
        assert!(lab.run(0).is_err()); assert!(lab.run(1001).is_err());
        for _ in 0..10 { lab.run(1000).unwrap(); }
        assert_eq!(state(&lab)["steps"], MAX_STEPS);
        assert!(state(&lab)["stopped"].as_str().unwrap().contains("budget"));
    }
}
