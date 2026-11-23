//! DRAFT v1.0 compatibility tests (issue #481, docs/CONTRACT_V1.md). They run the real binary and pin what a
//! caller can depend on today: the exact `ounce.run.v1` and `ounce.trace.v1` keys and JSON types, the meaning of the
//! `exit_code` field, the process exit statuses of `run`, `run-nro`, `run-elf`, `trace-diff`, and the CLI flags
//! `--max-frames`, `--sdmc`, `--timeout` and `--json`.
//!
//! The key sets live in `tests/contract/*.json`. These tests are the *gate*, not the freeze: whether any of this
//! is frozen is an owner decision (docs/CONTRACT_V1.md, "Owner questions"). Changing a baseline file is how a
//! contract change is made on purpose; a test failing without a baseline change is a regression.
//!
//! Programs are tiny NROs and ELFs assembled in this file (header branch to 0x80, one PT_LOAD), plus the in-repo
//! fixtures. Nothing needs a host window, a sound device or a C compiler.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_ounce-core");
const ROOT: &str = env!("CARGO_MANIFEST_DIR");

// ---------------------------------------------------------------------------------------------------------------
// Fixture construction

fn scratch(tag: &str) -> PathBuf {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ounce_contract_{tag}_{}_{}",
        std::process::id(),
        CALLS.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn words(code: &[u32]) -> Vec<u8> {
    code.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// A minimal NRO: the header's first word branches to 0x80, where `code` starts at guest 0x8000_0080.
fn nro(code: &[u32]) -> Vec<u8> {
    let text = words(code);
    let total = 0x80 + text.len();
    let mut b = vec![0u8; total];
    b[0..4].copy_from_slice(&0x1400_0020u32.to_le_bytes());
    b[0x10..0x14].copy_from_slice(b"NRO0");
    b[0x18..0x1c].copy_from_slice(&(total as u32).to_le_bytes());
    b[0x24..0x28].copy_from_slice(&(total as u32).to_le_bytes());
    b[0x80..].copy_from_slice(&text);
    b
}

/// A minimal static AArch64 Linux ELF with one executable PT_LOAD at 0x400000 whose entry (0x400078) is `code`.
fn elf(code: &[u32]) -> Vec<u8> {
    let text = words(code);
    let total = (0x78 + text.len()) as u64;
    let mut b = Vec::new();
    b.extend_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1, 0]);
    b.extend_from_slice(&[0u8; 8]);
    b.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    b.extend_from_slice(&183u16.to_le_bytes()); // EM_AARCH64
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&0x40_0078u64.to_le_bytes()); // entry
    b.extend_from_slice(&64u64.to_le_bytes()); // phoff
    b.extend_from_slice(&0u64.to_le_bytes()); // shoff
    b.extend_from_slice(&0u32.to_le_bytes()); // flags
    for v in [64u16, 56, 1, 0, 0, 0] {
        b.extend_from_slice(&v.to_le_bytes()); // ehsize, phentsize, phnum, shentsize, shnum, shstrndx
    }
    b.extend_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    b.extend_from_slice(&5u32.to_le_bytes()); // R+X
    for v in [0u64, 0x40_0000, 0x40_0000, total, total, 0x1000] {
        b.extend_from_slice(&v.to_le_bytes()); // offset, vaddr, paddr, filesz, memsz, align
    }
    assert_eq!(b.len(), 0x78);
    b.extend_from_slice(&text);
    b
}

/// Static AArch64 Linux ELF with an unstripped `.symtab` + `.strtab` for fault symbolication tests (#475).
fn elf_with_symtab(code: &[u32], syms: &[(&str, u64, u64)]) -> Vec<u8> {
    let mut b = elf(code);
    // Align to 8 bytes before appending .strtab, .symtab, and 3 section headers (NULL, SYMTAB, STRTAB).
    while !b.len().is_multiple_of(8) {
        b.push(0);
    }
    let strtab_off = b.len() as u64;
    b.push(0); // empty string at index 0
    let mut name_offsets = Vec::with_capacity(syms.len());
    for &(name, _, _) in syms {
        name_offsets.push((b.len() as u64 - strtab_off) as u32);
        b.extend_from_slice(name.as_bytes());
        b.push(0);
    }
    let strtab_size = b.len() as u64 - strtab_off;
    while !b.len().is_multiple_of(8) {
        b.push(0);
    }
    let symtab_off = b.len() as u64;
    b.extend_from_slice(&[0u8; 24]); // STN_UNDEF
    for (&(_, addr, size), &name_off) in syms.iter().zip(&name_offsets) {
        b.extend_from_slice(&name_off.to_le_bytes()); // st_name
        b.push((1 << 4) | 2); // STB_GLOBAL | STT_FUNC
        b.push(0); // st_other
        b.extend_from_slice(&1u16.to_le_bytes()); // st_shndx (non-zero)
        b.extend_from_slice(&addr.to_le_bytes()); // st_value
        b.extend_from_slice(&size.to_le_bytes()); // st_size
    }
    let symtab_size = b.len() as u64 - symtab_off;
    let shoff = b.len() as u64;
    let push_sh = |buf: &mut Vec<u8>, sh_type: u32, offset: u64, size: u64, link: u32, entsize: u64| {
        buf.extend_from_slice(&0u32.to_le_bytes()); // sh_name
        buf.extend_from_slice(&sh_type.to_le_bytes());
        buf.extend_from_slice(&0u64.to_le_bytes()); // sh_flags
        buf.extend_from_slice(&0u64.to_le_bytes()); // sh_addr
        buf.extend_from_slice(&offset.to_le_bytes());
        buf.extend_from_slice(&size.to_le_bytes());
        buf.extend_from_slice(&link.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // sh_info
        buf.extend_from_slice(&8u64.to_le_bytes()); // sh_addralign
        buf.extend_from_slice(&entsize.to_le_bytes());
    };
    push_sh(&mut b, 0, 0, 0, 0, 0); // [0] SHT_NULL
    push_sh(&mut b, 2, symtab_off, symtab_size, 2, 24); // [1] SHT_SYMTAB -> link 2 (.strtab)
    push_sh(&mut b, 3, strtab_off, strtab_size, 0, 0); // [2] SHT_STRTAB
    // Patch e_shoff (offset 40), e_shentsize (58), e_shnum (60).
    b[40..48].copy_from_slice(&shoff.to_le_bytes());
    b[58..60].copy_from_slice(&64u16.to_le_bytes());
    b[60..62].copy_from_slice(&3u16.to_le_bytes());
    b
}

/// `_start` pushes an AAPCS64 frame record and calls `crash_fn`, which pushes a frame record and loads `0x10` (#475).
const SYMBOLICATED_FAULT_CODE: [u32; 8] = [
    0xA9BF_7BFD, // _start+0x0:   stp x29, x30, [sp, #-16]!
    0x9100_03FD, // _start+0x4:   mov x29, sp
    0x9400_0002, // _start+0x8:   bl crash_fn (+8 bytes -> 0x400088)
    0xD503_201F, // _start+0xc:   nop
    0xA9BF_7BFD, // crash_fn+0x0: stp x29, x30, [sp, #-16]!
    0x9100_03FD, // crash_fn+0x4: mov x29, sp
    0xD280_0201, // crash_fn+0x8: movz x1, #0x10
    0xF940_0020, // crash_fn+0xc: ldr x0, [x1] -> memory_fault at 0x400094
];

// Hand-assembled A64 words (the encodings are listed in docs/CONTRACT_V1.md).
const MOVZ_W0_0: u32 = 0x5280_0000;
const SVC_EXIT_PROCESS: u32 = 0xD400_00E1; // svc #7
const SVC_BREAK: u32 = 0xD400_04C1; // svc #0x26
const SVC_UNKNOWN: u32 = 0xD400_1FE1; // svc #0xff
const SVC_PRESENT: u32 = 0xD400_0301; // svc #0x18
const BRK_0: u32 = 0xD420_0000;
const UDF_0: u32 = 0x0000_0000;
const B_SELF: u32 = 0x1400_0000;
/// `movz x1,#0x10; ldr x0,[x1]`: a load from unmapped memory.
const LOAD_UNMAPPED: [u32; 2] = [0xD280_0201, 0xF940_0020];
/// svcBreak with `info1` pointing at a readable word (the last word of the program, 0x2A) and `info2` = 4, so the
/// report's `break_result` is that word: `movz x1,#0x94; movk x1,#0x8000,lsl 16; movz x2,#4; movz x0,#0; svc; .word 0x2A`.
const BREAK_WITH_RESULT: [u32; 6] = [0xD280_1281, 0xF2B0_0001, 0xD280_0082, 0xD280_0000, SVC_BREAK, 0x2A];
/// Fills the first framebuffer pixel and presents, forever: `movz x4,#0; movk x4,#0x9000,lsl 16; movz x2,#0x1234;
/// str w2,[x4]; svc #0x18; b .-4`.
const PRESENT_LOOP: [u32; 6] = [
    0xD280_0004,
    0xF2B2_0004,
    0xD282_4682,
    0xB900_0082,
    SVC_PRESENT,
    0x17FF_FFFE,
];
/// Connects to `sm:`, calls `GetService("apm")` (CMIF cmd 1), then triggers an unhandled `svc #0xff` so exit-14
/// reports `sm_service: "apm"` and `cmif_cmd_id: 1` (#475).
const SM_THEN_BAD_SVC: [u32; 16] = [
    0xD280_1781, // movz x1, #0xbc  ("sm:\0" at 0x8000_00bc)
    0xF2B0_0001, // movk x1, #0x8000, lsl #16
    0xD400_03E1, // svc #0x1f (svcConnectToNamedPort) -> x1 = sm handle
    0xD53B_D069, // mrs x9, tpidrro_el0 (TLS)
    0x5288_CA6A, // movz w10, #0x4653 ("SF")
    0x72A9_286A, // movk w10, #0x4943, lsl #16 ("CI")
    0xB900_112A, // str w10, [x9, #0x10] (CMIF_IN_MAGIC)
    0x5280_002A, // movz w10, #1 (GetService)
    0xB900_192A, // str w10, [x9, #0x18] (cmd_id)
    0x528E_0C2A, // movz w10, #0x7061 ("ap")
    0x72A0_0DAA, // movk w10, #0x006d, lsl #16 ("m\0")
    0xB900_212A, // str w10, [x9, #0x20] ("apm\0")
    0xAA01_03E0, // mov x0, x1
    0xD400_0421, // svc #0x21 (svcSendSyncRequest)
    SVC_UNKNOWN, // svc #0xff (unhandled_svc -> exit 14)
    0x003A_6D73, // "sm:\0"
];

// ---------------------------------------------------------------------------------------------------------------
// Running the binary

struct Outcome {
    /// Process exit status.
    status: i32,
    /// The `--report-json` file, if the run wrote one.
    report: Option<Value>,
    stderr: String,
}

fn finish(output: Output, report: &Path) -> Outcome {
    Outcome {
        status: output
            .status
            .code()
            .expect("the process ended by exit, not by a signal"),
        report: std::fs::read(report)
            .ok()
            .map(|b| serde_json::from_slice(&b).expect("the report is JSON")),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// `ounce-core <command> <program> <args...> --report-json=<tmp>` with extra environment variables.
fn run_program(tag: &str, command: &str, program: &Path, args: &[&str], env: &[(&str, &str)]) -> Outcome {
    let dir = scratch(tag);
    let report = dir.join("report.json");
    let mut cmd = Command::new(BIN);
    cmd.arg(command)
        .arg(program)
        .args(args)
        .arg(format!("--report-json={}", report.display()));
    for (k, v) in env {
        cmd.env(k, v);
    }
    let outcome = finish(cmd.output().unwrap(), &report);
    let _ = std::fs::remove_dir_all(&dir);
    outcome
}

/// Writes `bytes` to a scratch file and runs it with `run-nro` (headless, deterministic) or `run-elf`.
fn run_bytes(tag: &str, command: &str, bytes: &[u8], args: &[&str], env: &[(&str, &str)]) -> Outcome {
    let dir = scratch(tag);
    let file = dir.join(if command == "run-elf" { "prog.elf" } else { "prog.nro" });
    std::fs::write(&file, bytes).unwrap();
    let outcome = run_program(tag, command, &file, args, env);
    let _ = std::fs::remove_dir_all(&dir);
    outcome
}

fn run_nro_bytes(tag: &str, code: &[u32], extra: &[&str], env: &[(&str, &str)]) -> Outcome {
    let mut args = vec!["--present=headless", "--deterministic"];
    args.extend_from_slice(extra);
    run_bytes(tag, "run-nro", &nro(code), &args, env)
}

fn fixture(rel: &str) -> PathBuf {
    Path::new(ROOT).join(rel)
}

fn command(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap()
}

// ---------------------------------------------------------------------------------------------------------------
// Representative runs: every stop_kind a test can reach, on both entry points that write reports.

struct Case {
    name: &'static str,
    /// Process exit status the contract promises for this run.
    status: i32,
    /// `(stop_kind, exit_code)` the report must carry.
    stop: (&'static str, i64),
    outcome: Outcome,
}

fn cases() -> &'static [Case] {
    static CASES: OnceLock<Vec<Case>> = OnceLock::new();
    CASES.get_or_init(|| {
        let mut v = Vec::new();
        let mut add = |name, status, stop, outcome| {
            v.push(Case {
                name,
                status,
                stop,
                outcome,
            })
        };
        // run-nro: every non-clean stop is process status 1; a clean exit or the frame limit is 0.
        add(
            "run-nro clean exit",
            0,
            ("clean_exit", 0),
            run_nro_bytes("exit", &[MOVZ_W0_0, SVC_EXIT_PROCESS], &[], &[]),
        );
        add(
            "run-nro frame limit",
            0,
            ("frame_limit", 0),
            run_nro_bytes("frames", &PRESENT_LOOP, &["--max-frames", "2"], &[]),
        );
        add(
            "run-nro udf",
            1,
            ("unimplemented_insn", 10),
            run_nro_bytes("udf", &[UDF_0], &[], &[]),
        );
        add(
            "run-nro unmapped load",
            1,
            ("memory_fault", 11),
            run_nro_bytes("fault", &LOAD_UNMAPPED, &[], &[]),
        );
        add(
            "run-nro brk",
            1,
            ("cpu_error", 12),
            run_nro_bytes("brk", &[BRK_0], &[], &[]),
        );
        add(
            "run-nro svcBreak",
            1,
            ("svc_break", 13),
            run_nro_bytes("break", &[MOVZ_W0_0, SVC_BREAK], &[], &[]),
        );
        add(
            "run-nro svcBreak with result",
            1,
            ("svc_break", 13),
            run_nro_bytes("breakres", &BREAK_WITH_RESULT, &[], &[]),
        );
        add(
            "run-nro unknown svc",
            1,
            ("unhandled_svc", 14),
            run_nro_bytes("badsvc", &[SVC_UNKNOWN], &[], &[]),
        );
        add(
            "run-nro unknown svc after sm",
            1,
            ("unhandled_svc", 14),
            run_nro_bytes("smbadsvc", &SM_THEN_BAD_SVC, &[], &[]),
        );
        add(
            "run-nro budget",
            1,
            ("budget_exhausted", 16),
            run_nro_bytes("loop", &[B_SELF], &[], &[("OUNCE_NRO_BUDGET", "5000")]),
        );
        let audren = fixture("tests/fixtures/nro/audren/audren.nro");
        add(
            "run-nro audio fixture",
            0,
            ("clean_exit", 0),
            run_program(
                "audren",
                "run-nro",
                &audren,
                &["--present=headless", "--deterministic"],
                &[],
            ),
        );
        // run-elf: the guest's own status on a clean exit, 70..73 for engine stops.
        let elf_args = ["--deterministic"];
        let bin = |name: &str| fixture(&format!("tests/linux-elf/bin/{name}.elf"));
        add(
            "run-elf exit 42",
            42,
            ("clean_exit", 0),
            run_program("e42", "run-elf", &bin("exitcode"), &elf_args, &[]),
        );
        add(
            "run-elf unknown syscall",
            70,
            ("unhandled_svc", 14),
            run_program("e70", "run-elf", &bin("badsyscall"), &elf_args, &[]),
        );
        add(
            "run-elf unmapped load",
            71,
            ("memory_fault", 11),
            run_bytes("e71a", "run-elf", &elf(&LOAD_UNMAPPED), &elf_args, &[]),
        );
        add(
            "run-elf symbolicated fault",
            71,
            ("memory_fault", 11),
            run_bytes(
                "e71sym",
                "run-elf",
                &elf_with_symtab(
                    &SYMBOLICATED_FAULT_CODE,
                    &[("_start", 0x40_0078, 16), ("crash_fn", 0x40_0088, 16)],
                ),
                &elf_args,
                &[],
            ),
        );
        add(
            "run-elf udf",
            71,
            ("unimplemented_insn", 10),
            run_bytes("e71b", "run-elf", &elf(&[UDF_0]), &elf_args, &[]),
        );
        add(
            "run-elf not an ELF",
            72,
            ("loader_error", 17),
            run_bytes("e72", "run-elf", b"not an elf", &elf_args, &[]),
        );
        add(
            "run-elf budget",
            73,
            ("budget_exhausted", 16),
            run_bytes("e73", "run-elf", &elf(&[B_SELF]), &["--max-insns=1000"], &[]),
        );
        v
    })
}

fn baseline(file: &str) -> Value {
    serde_json::from_slice(&std::fs::read(fixture(&format!("tests/contract/{file}"))).unwrap()).unwrap()
}

fn json_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Allowed JSON types per key, from a baseline object like `{"pc": ["null", "number"]}`.
fn allowed(spec: &Value) -> BTreeMap<String, BTreeSet<String>> {
    spec.as_object()
        .unwrap()
        .iter()
        .map(|(k, types)| {
            (
                k.clone(),
                types
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| t.as_str().unwrap().to_string())
                    .collect(),
            )
        })
        .collect()
}

/// Checks `docs` (labelled documents of one kind) against the baseline `spec`: exactly the same keys, every value of
/// a listed type, and every listed type seen somewhere, so the baseline cannot promise more than a run shows.
fn check_documents(what: &str, docs: &[(String, &Value)], spec: &Value, baseline_file: &str) {
    let allowed = allowed(spec);
    let mut seen: BTreeMap<&str, BTreeSet<&'static str>> = BTreeMap::new();
    for (label, doc) in docs {
        let object = doc
            .as_object()
            .unwrap_or_else(|| panic!("{what} [{label}] is not a JSON object: {doc}"));
        let keys: BTreeSet<&String> = object.keys().collect();
        let expected: BTreeSet<&String> = allowed.keys().collect();
        let missing: Vec<_> = expected.difference(&keys).collect();
        let extra: Vec<_> = keys.difference(&expected).collect();
        assert!(
            missing.is_empty(),
            "{what} [{label}] lost key(s) {missing:?}: removing or renaming a key breaks the additive-only contract (docs/CONTRACT_V1.md)"
        );
        assert!(
            extra.is_empty(),
            "{what} [{label}] has new key(s) {extra:?}: if this is intended, add them to tests/contract/{baseline_file} and the schema \
             doc in the same change (docs/CONTRACT_V1.md, 'Additive-only rule')"
        );
        for (key, value) in object {
            let t = json_type(value);
            assert!(
                allowed[key].contains(t),
                "{what} [{label}] key `{key}` is {t} ({value}); the contract allows {:?}: changing a type breaks it",
                allowed[key]
            );
            seen.entry(key.as_str()).or_default().insert(t);
        }
    }
    for (key, types) in &allowed {
        let observed: BTreeSet<String> = seen
            .get(key.as_str())
            .into_iter()
            .flatten()
            .map(|t| t.to_string())
            .collect();
        assert_eq!(
            &observed, types,
            "{what} key `{key}`: the representative runs produced {observed:?} but the baseline lists {types:?}; either a run no longer \
             reaches a case or the baseline promises a type nothing produces"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// ounce.run.v1

#[test]
fn run_v1_keys_and_json_types_are_pinned() {
    let base = baseline("run-v1-keys.json");
    let docs: Vec<(String, &Value)> = cases()
        .iter()
        .map(|c| {
            (
                c.name.to_string(),
                c.outcome
                    .report
                    .as_ref()
                    .unwrap_or_else(|| panic!("{}: no report written", c.name)),
            )
        })
        .collect();
    check_documents("ounce.run.v1 report", &docs, &base["keys"], "run-v1-keys.json");
    for (label, doc) in &docs {
        assert_eq!(doc["schema"], base["schema"], "[{label}]");
        assert_eq!(doc["engine"], "ounce-core", "[{label}]");
        assert_eq!(doc["engine_version"], env!("CARGO_PKG_VERSION"), "[{label}]");
    }
}

#[test]
fn run_v1_exit_code_follows_stop_kind() {
    let base = baseline("run-v1-keys.json");
    let table = base["stop_kinds"].as_object().unwrap();
    let mut reached = BTreeSet::new();
    for c in cases() {
        let report = c.outcome.report.as_ref().unwrap();
        let (kind, exit_code) = (
            report["stop_kind"].as_str().unwrap(),
            report["exit_code"].as_i64().unwrap(),
        );
        assert_eq!((kind, exit_code), c.stop, "{}: {report}", c.name);
        assert_eq!(
            table[kind].as_i64(),
            Some(exit_code),
            "{}: `{kind}` must map to the baseline's exit_code",
            c.name
        );
        reached.insert(kind.to_string());
    }
    let unreachable: BTreeSet<String> = base["stop_kinds_not_reachable_in_tests"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let all: BTreeSet<String> = table.keys().cloned().collect();
    assert_eq!(
        reached.union(&unreachable).cloned().collect::<BTreeSet<_>>(),
        all,
        "every stop_kind is either tested or listed as not reachable"
    );
    assert!(
        reached.is_disjoint(&unreachable),
        "a stop_kind listed as not reachable was reached: update the baseline"
    );
    // The exit codes the contract names: 0 and 10..=17.
    let codes: BTreeSet<i64> = table.values().map(|v| v.as_i64().unwrap()).collect();
    assert_eq!(codes, [0, 10, 11, 12, 13, 14, 15, 16, 17].into_iter().collect());
}

#[test]
fn run_v1_stop_details_follow_the_stop() {
    let by_name = |name: &str| {
        cases()
            .iter()
            .find(|c| c.name == name)
            .unwrap()
            .outcome
            .report
            .as_ref()
            .unwrap()
    };
    assert_eq!(by_name("run-nro udf")["opcode"], 0);
    assert!(by_name("run-nro unmapped load")["fault_addr"].as_u64() == Some(0x10));
    assert_eq!(by_name("run-nro svcBreak with result")["break_result"], 0x2A);
    assert!(by_name("run-nro svcBreak")["break_result"].is_null());
    assert!(by_name("run-nro unknown svc")["pc"].is_number());
    assert!(by_name("run-nro unknown svc")["sm_service"].is_null());
    assert!(by_name("run-nro unknown svc")["cmif_cmd_id"].is_null());
    let sm_bad = by_name("run-nro unknown svc after sm");
    assert_eq!(sm_bad["svc_id"], 0xFF);
    assert_eq!(sm_bad["sm_service"], "apm");
    assert_eq!(sm_bad["cmif_cmd_id"], 1);
    assert!(
        sm_bad["message"]
            .as_str()
            .is_some_and(|m| m.contains("apm") && m.contains("cmif cmd 1")),
        "expected sm service and cmd in stop message, got {:?}",
        sm_bad["message"]
    );
    assert!(by_name("run-nro clean exit")["sm_service"].is_null());
    assert!(by_name("run-nro clean exit")["cmif_cmd_id"].is_null());
    assert_eq!(by_name("run-elf exit 42")["guest_exit_status"], 42);
    assert_eq!(
        by_name("run-elf exit 42")["exit_code"],
        0,
        "exit_code is how the engine stopped; the guest's own status is separate"
    );
    assert_eq!(by_name("run-elf unknown syscall")["svc_id"], 280);
    assert!(by_name("run-elf unknown syscall")["guest_exit_status"].is_null());
    assert!(by_name("run-elf not an ELF")["message"].is_string());
    let sym = by_name("run-elf symbolicated fault");
    assert_eq!(sym["fault_addr"], 0x10);
    assert_eq!(sym["fault_symbol"], "crash_fn+0xc");
    assert!(
        sym["backtrace"]
            .as_str()
            .is_some_and(|bt| bt.contains("crash_fn+0xc") && bt.contains("_start+0x8")),
        "expected symbolicated FP backtrace, got {:?}",
        sym["backtrace"]
    );
    assert!(
        sym["message"].as_str().is_some_and(|m| m.contains("crash_fn+0xc")),
        "expected symbol in fault message, got {:?}",
        sym["message"]
    );
    assert!(by_name("run-elf exit 42")["fault_symbol"].is_null());
    assert!(by_name("run-elf exit 42")["backtrace"].is_null());
    let frames = by_name("run-nro frame limit");
    assert_eq!(frames["frames_presented"], 2);
    assert!(
        frames["framebuffer_hash"]
            .as_str()
            .is_some_and(|h| h.starts_with("fnv1a64:")
                && h.len() == 8 + 16
                && h[8..].bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    );
    let audio = by_name("run-nro audio fixture");
    assert_eq!(audio["audio_frames_mixed"], 2640);
    assert!(audio["audio_hash"].as_str().is_some_and(|h| h.starts_with("fnv1a64:")));
}

// ---------------------------------------------------------------------------------------------------------------
// Process exit statuses

#[test]
fn run_and_run_nro_process_exit_statuses() {
    for c in cases() {
        assert_eq!(c.outcome.status, c.status, "{}: stderr: {}", c.name, c.outcome.stderr);
    }
}

#[test]
fn run_nro_usage_and_input_errors_are_statuses_1_and_2() {
    let missing = std::env::temp_dir().join(format!("ounce_contract_missing_{}.nro", std::process::id()));
    assert_eq!(
        code(&command(&["run-nro", missing.to_str().unwrap(), "--present=headless"])),
        1,
        "missing input file"
    );
    assert_eq!(code(&command(&["run-nro"])), 2, "no program named");
    assert_eq!(code(&command(&["no-such-command"])), 2, "unknown command");
    let dir = scratch("usage");
    let file = dir.join("bad.nro");
    std::fs::write(&file, [0u8; 128]).unwrap();
    let out = command(&["run-nro", file.to_str().unwrap(), "--present=headless"]);
    assert_eq!(
        code(&out),
        1,
        "a loader failure on run-nro is status 1 (it writes no report)"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("[Loader Error]"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_nro_load_failure_writes_no_report() {
    let dir = scratch("noreport");
    let (file, report) = (dir.join("bad.nro"), dir.join("r.json"));
    std::fs::write(&file, [0u8; 128]).unwrap();
    let out = command(&[
        "run-nro",
        file.to_str().unwrap(),
        "--present=headless",
        &format!("--report-json={}", report.display()),
    ]);
    assert_eq!(code(&out), 1);
    assert!(
        !report.exists(),
        "today a run-nro load failure has no ounce.run.v1 report (stop_kind loader_error is run-elf only)"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_elf_usage_errors_are_status_2() {
    assert_eq!(code(&command(&["run-elf"])), 2, "no ELF named");
    assert_eq!(code(&command(&["run-elf", "--no-such-flag"])), 2, "unknown option");
    assert_eq!(
        code(&command(&["run-elf", "x.elf", "--max-insns=abc"])),
        2,
        "bad --max-insns"
    );
}

#[test]
fn trace_diff_exit_statuses_are_0_1_2() {
    let (traces, _) = recorded_traces("status");
    let (a, b) = (&traces[0], &traces[1]);
    assert_eq!(
        code(&command(&["trace-diff", a.to_str().unwrap(), a.to_str().unwrap()])),
        0,
        "no divergence"
    );
    assert_eq!(
        code(&command(&["trace-diff", a.to_str().unwrap(), b.to_str().unwrap()])),
        1,
        "divergence"
    );
    assert_eq!(
        code(&command(&[
            "trace-diff",
            a.to_str().unwrap(),
            "/nonexistent/ounce_contract.json"
        ])),
        2,
        "unreadable input"
    );
    assert_eq!(code(&command(&["trace-diff"])), 2, "missing arguments");
}

#[test]
fn diverge_exit_statuses_that_need_no_compiler_are_status_2() {
    assert_eq!(code(&command(&["diverge"])), 2, "no program named");
    assert_eq!(
        code(&command(&["diverge", "/nonexistent/ounce_contract.nro"])),
        2,
        "not a file"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Flags: --max-frames --sdmc --timeout --json

#[test]
fn max_frames_stops_a_run_after_n_presented_frames_in_both_spellings() {
    for n in [1usize, 3] {
        for spelling in [
            vec!["--max-frames".to_string(), n.to_string()],
            vec![format!("--max-frames={n}")],
        ] {
            let args: Vec<&str> = spelling.iter().map(String::as_str).collect();
            let out = run_nro_bytes("maxframes", &PRESENT_LOOP, &args, &[]);
            let report = out.report.as_ref().unwrap();
            assert_eq!(out.status, 0, "{spelling:?}: {}", out.stderr);
            assert_eq!(report["stop_kind"], "frame_limit", "{spelling:?}: {report}");
            assert_eq!(report["frames_presented"], n, "{spelling:?}: {report}");
        }
    }
}

#[test]
fn max_frames_rejects_a_value_that_is_not_a_positive_integer() {
    let dir = scratch("maxbad");
    let file = dir.join("p.nro");
    std::fs::write(&file, nro(&PRESENT_LOOP)).unwrap();
    for value in ["0", "-1", "abc", ""] {
        let out = command(&[
            "run-nro",
            file.to_str().unwrap(),
            "--present=headless",
            &format!("--max-frames={value}"),
        ]);
        assert_eq!(code(&out), 2, "--max-frames={value}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("--max-frames expects a positive integer"),
            "{value}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sdmc_directory_is_visible_to_the_guest_and_never_modified() {
    let dir = scratch("sdmc");
    let host = dir.join("sd");
    std::fs::create_dir_all(&host).unwrap();
    std::fs::write(host.join("host.txt"), "from host\n").unwrap();
    let out = command(&[
        "run-nro",
        fixture("tests/fixtures/nro/sdmc/sdmc.nro").to_str().unwrap(),
        "--present=headless",
        &format!("--sdmc={}", host.display()),
    ]);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("ounce-sdmc result=0x00000EEE"),
        "the guest read host.txt and finished every step"
    );
    let names: Vec<_> = std::fs::read_dir(&host)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(
        names,
        ["host.txt"],
        "without --sdmc-writeback the host folder is untouched"
    );
    assert_eq!(std::fs::read_to_string(host.join("host.txt")).unwrap(), "from host\n");
    // Only the `--sdmc=<dir>` spelling exists; a bare `--sdmc` is not an option.
    let bare = command(&[
        "run-nro",
        fixture("tests/fixtures/nro/sdmc/sdmc.nro").to_str().unwrap(),
        "--present=headless",
        "--sdmc",
    ]);
    assert_eq!(code(&bare), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `--timeout` and `--json` are not options of `run` / `run-nro` in ounce-core (the roadmap places them on
/// `ounce-hb run`). What ounce-core does today: `run` refuses them with status 2, and a program can still receive
/// them after `--`; `ui --timeout SECS` exists; `--json <path>` is an option of `diverge`, `trace-diff` and `census`.
#[test]
fn timeout_and_json_are_not_run_flags_on_ounce_core() {
    let dir = scratch("dx");
    let file = dir.join("p.nro");
    std::fs::write(&file, nro(&[MOVZ_W0_0, SVC_EXIT_PROCESS])).unwrap();
    let file = file.to_str().unwrap();
    for flag in [
        &["--timeout", "5"][..],
        &["--timeout=5"],
        &["--json"],
        &["--json", "out.json"],
    ] {
        let out = command(&[&["run-nro", file, "--present=headless"], flag].concat());
        assert_eq!(code(&out), 2, "{flag:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("unknown option"),
            "{flag:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let passed = command(&["run-nro", file, "--present=headless", "--", "--timeout", "5", "--json"]);
    assert_eq!(code(&passed), 0, "after `--` they are guest arguments");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ui_timeout_takes_seconds_and_diverge_json_takes_a_path() {
    let bad = command(&["ui", "--timeout", "abc"]);
    assert_eq!(
        (
            code(&bad),
            String::from_utf8_lossy(&bad.stderr).contains("--timeout must be seconds")
        ),
        (2, true)
    );
    let none = command(&["ui", "--timeout"]);
    assert_eq!(
        (
            code(&none),
            String::from_utf8_lossy(&none.stderr).contains("--timeout needs a value")
        ),
        (2, true)
    );
    let json = command(&["diverge", "--json"]);
    assert_eq!(
        (
            code(&json),
            String::from_utf8_lossy(&json.stderr).contains("--json needs a path")
        ),
        (2, true)
    );
}

// ---------------------------------------------------------------------------------------------------------------
// ounce.trace.v1

/// Two traces of the same tiny program that differ in the opcode of step 1, written by the real binary and then
/// edited, so `trace-diff` has something to find. Returns the two paths and the scratch directory.
fn recorded_traces(tag: &str) -> (Vec<PathBuf>, PathBuf) {
    let dir = scratch(tag);
    let program = dir.join("p.nro");
    std::fs::write(&program, nro(&LOAD_UNMAPPED)).unwrap();
    let (a, b) = (dir.join("a.json"), dir.join("b.json"));
    let out = Command::new(BIN)
        .args([
            "run-nro",
            program.to_str().unwrap(),
            "--present=headless",
            "--deterministic",
        ])
        .env("OUNCE_TRACE_SUMMARY", &a)
        .env("OUNCE_TRACE_STEPS", "1024") // do not inherit the caller's limit
        .output()
        .unwrap();
    assert_eq!(
        code(&out),
        1,
        "the program faults on purpose; the trace is written on every stop"
    );
    let mut edited: Value = serde_json::from_slice(&std::fs::read(&a).unwrap()).unwrap();
    edited["steps"][1]["opcode"] = Value::String("0xd503201f".into());
    std::fs::write(&b, serde_json::to_vec(&edited).unwrap()).unwrap();
    (vec![a, b], dir)
}

#[test]
fn trace_v1_keys_and_json_types_are_pinned() {
    let base = baseline("trace-v1-keys.json");
    let (traces, dir) = recorded_traces("keys");
    let (a, b) = (&traces[0], &traces[1]);
    let trace: Value = serde_json::from_slice(&std::fs::read(a).unwrap()).unwrap();
    assert_eq!(trace["schema"], base["schema"]);
    assert_eq!(trace["kind"], "trace");
    check_documents(
        "ounce.trace.v1 trace",
        &[("trace".to_string(), &trace)],
        &base["trace"],
        "trace-v1-keys.json",
    );
    let steps: Vec<(String, &Value)> = trace["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (format!("step {}", s["i"]), s))
        .collect();
    check_documents("ounce.trace.v1 step", &steps, &base["step"], "trace-v1-keys.json");

    let (matched, differ, reg_trace, reg_differ) = (
        dir.join("match.json"),
        dir.join("differ.json"),
        dir.join("reg.json"),
        dir.join("reg_differ.json"),
    );
    assert_eq!(
        code(&command(&[
            "trace-diff",
            a.to_str().unwrap(),
            a.to_str().unwrap(),
            "--json",
            matched.to_str().unwrap()
        ])),
        0
    );
    assert_eq!(
        code(&command(&[
            "trace-diff",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--json",
            differ.to_str().unwrap()
        ])),
        1
    );
    let read = |p: &Path| -> Value { serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap() };
    // A register divergence fills `name`: x1 differs at step 1.
    let mut edited = trace.clone();
    edited["steps"][1]["regs"]["x1"] = Value::String("0x0000000000000011".into());
    std::fs::write(&reg_trace, serde_json::to_vec(&edited).unwrap()).unwrap();
    assert_eq!(
        code(&command(&[
            "trace-diff",
            a.to_str().unwrap(),
            reg_trace.to_str().unwrap(),
            "--json",
            reg_differ.to_str().unwrap()
        ])),
        1
    );
    let (m, d, r) = (read(&matched), read(&differ), read(&reg_differ));
    assert_eq!(
        (r["field"].as_str(), r["name"].as_str(), r["actual"].as_str()),
        (Some("register"), Some("x1"), Some("0x0000000000000011"))
    );
    assert_eq!(
        (m["kind"].as_str(), d["kind"].as_str()),
        (Some("divergence"), Some("divergence"))
    );
    assert_eq!((m["match"].as_bool(), d["match"].as_bool()), (Some(true), Some(false)));
    assert_eq!((d["step"].as_u64(), d["field"].as_str()), (Some(1), Some("opcode")));
    check_documents(
        "ounce.trace.v1 divergence",
        &[
            ("match".to_string(), &m),
            ("divergence".to_string(), &d),
            ("register".to_string(), &r),
        ],
        &base["divergence"],
        "trace-v1-keys.json",
    );
    let window: Vec<(String, &Value)> = d["window"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| (format!("window {}", w["i"]), w))
        .collect();
    assert!(!window.is_empty(), "the divergence at step 1 has step 0 in its window");
    check_documents(
        "ounce.trace.v1 window entry",
        &window,
        &base["window_entry"],
        "trace-v1-keys.json",
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn trace_v1_numbers_are_hex_strings_and_counts_are_integers() {
    let (traces, dir) = recorded_traces("hex");
    let trace: Value = serde_json::from_slice(&std::fs::read(&traces[0]).unwrap()).unwrap();
    let first = &trace["steps"][0];
    assert_eq!(first["pc"], "0x0000000080000000", "pc is 16 hex digits");
    assert_eq!(
        first["opcode"].as_str().map(str::len),
        Some(10),
        "opcode is 0x + 8 hex digits"
    );
    assert_eq!(trace["steps"][1]["regs"]["x1"], "0x0000000000000010");
    assert_eq!(
        (
            trace["steps_executed"].as_u64(),
            trace["steps_recorded"].as_u64(),
            trace["truncated"].as_bool()
        ),
        (Some(2), Some(2), Some(false))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn report_json_dash_emits_pure_json_to_stdout() {
    let bin = fixture("tests/linux-elf/bin/exitcode.elf");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args([
            "run-elf",
            bin.to_str().unwrap(),
            "--deterministic",
            "--report-json=-",
            "--",
            "--guest-flag",
            "-v",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(42));
    let stdout = String::from_utf8(out.stdout).unwrap();
    let parsed: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("expected pure JSON on stdout, got error {e}: {stdout:?}"));
    assert_eq!(parsed["schema"], "ounce.run.v1");
    assert_eq!(parsed["stop_kind"], "clean_exit");
    assert_eq!(parsed["guest_exit_status"], 42);
}
