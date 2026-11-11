//! Wording and routing for the command line: what a bare invocation prints, how an unknown command is
//! reported, and what to do next when a run stops early. Pure text and no I/O, so every message is unit tested.

/// Every command word `main` dispatches on, used to suggest a correction for a typo.
pub const COMMANDS: &[&str] = &[
    "run", "run-nro", "run-elf", "ui", "lab", "diverge", "census", "inspect", "info", "help", "test", "demo", "play",
    "gfx", "3d", "gpu3d", "package", "demo-dir", "jit-selftest", "replay-instruction", "fuzz-replay",
];

pub const ISSUE_FORM: &str = "https://github.com/hffgzktbcc-blip/ounce-core/issues/new?template=emulation_bug.yml";

/// Why a run stopped, as far as the next step is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Break,
    UnhandledSvc,
    Halt,
    Fault,
    Budget,
}

/// What a bare `ounce-core` prints: three ways to start, and where the full list is.
pub fn getting_started(version: &str) -> String {
    format!(
        "Ounce Core {version}\n\n\
         Start here:\n  \
         ounce-core ui                     Check a program in the browser workbench (drop a .nro, compare engines)\n  \
         ounce-core run-nro <file.nro>     Run a Switch homebrew program and print where it stopped\n  \
         ounce-core run-elf <file>         Run a static AArch64 Linux program\n\n\
         If a run stops early, the output ends with the next step and how to report it.\n\
         All commands: ounce-core help\n"
    )
}

/// Message for a command word that is not recognised.
pub fn unknown_command(word: &str) -> String {
    let mut out = format!("error: unknown command '{word}'.");
    if let Some(close) = suggest(word) {
        out.push_str(&format!(" Did you mean '{close}'?"));
    }
    out.push_str("\nRun 'ounce-core help' for the list of commands.\n");
    out
}

/// Message for a command that needs a file and did not get one.
pub fn missing_path(command: &str, what: &str) -> String {
    format!("error: '{command}' needs {what}.\nExample: ounce-core {command} path/to/program\nRun 'ounce-core help' for all options.\n")
}

/// Message for an input path that does not exist.
pub fn file_not_found(path: &str) -> String {
    format!(
        "[Loader Error] Failed to read file '{path}': not found.\n\
         Check the path and the file name, then run the command again. Use an absolute path if the file is in another folder.\n"
    )
}

/// The closest known command, if one is near enough to be a plausible typo.
pub fn suggest(word: &str) -> Option<&'static str> {
    let word = word.to_ascii_lowercase();
    COMMANDS
        .iter()
        .map(|c| (distance(&word, c), *c))
        .filter(|(d, c)| *d <= 2 || (word.len() >= 3 && c.starts_with(&word)))
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let next = (row[j + 1] + 1).min(row[j] + 1).min(prev + usize::from(ca != cb));
            prev = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b.len()]
}

/// Printed after a run that did not reach `svcExitProcess`, so the failure ends with an action.
pub fn next_steps(stop: Stop, program: &str, engine: &str) -> String {
    let headline = match stop {
        Stop::Break => "The program stopped itself with svcBreak, which programs use to report a fatal error.",
        Stop::UnhandledSvc => "The program asked for a system service Ounce Core does not provide (see docs/svc-coverage.md).",
        Stop::Halt => "The CPU met an instruction Ounce Core does not implement. This is a gap in Ounce Core.",
        Stop::Fault => "The CPU stopped on an error. The [VM Error] line above names the address; 'Unimplemented instruction' means a gap in Ounce Core.",
        Stop::Budget => "The instruction budget ran out before the program exited.",
    };
    let try_next = match stop {
        Stop::Budget => format!("  Raise the limit:  OUNCE_NRO_BUDGET=500000000 ounce-core run-nro {program}"),
        Stop::Break => "  Read the lines above the break for the last service request, then compare engines:  ounce-core ui".to_string(),
        Stop::UnhandledSvc => "  Look up the call number in docs/svc-coverage.md; a missing service is a gap in Ounce Core.".to_string(),
        Stop::Halt | Stop::Fault => {
            format!("  Compare with the host CPU:  ounce-core diverge {program}   (Apple Silicon and other AArch64 hosts)")
        }
    };
    let report = match stop {
        Stop::Break | Stop::Budget => String::new(),
        _ => format!(
            "  Report it:        {ISSUE_FORM}\n                    Include: ounce-core run-nro {program} --cpu={engine}, your OS, `ounce-core --version`, and the stop line above.\n"
        ),
    };
    format!("\nRun stopped early. {headline}\n{try_next}\n{report}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typos_get_a_suggestion_and_nonsense_does_not() {
        assert_eq!(suggest("run-nr"), Some("run-nro"));
        assert_eq!(suggest("inpsect"), Some("inspect"));
        assert_eq!(suggest("RUN"), Some("run"));
        assert_eq!(suggest("frobnicate"), None);
    }

    #[test]
    fn unknown_command_names_the_word_and_points_to_help() {
        let text = unknown_command("run-nr");
        assert!(text.contains("unknown command 'run-nr'") && text.contains("Did you mean 'run-nro'?"));
        assert!(unknown_command("frobnicate").contains("ounce-core help"));
        assert!(!unknown_command("frobnicate").contains("Did you mean"));
    }

    #[test]
    fn bare_invocation_is_short_and_plain() {
        let text = getting_started("1.2.3");
        assert!(text.lines().count() < 14 && text.contains("ounce-core ui") && text.contains("1.2.3"));
        assert!(text.is_ascii());
    }

    #[test]
    fn every_stop_ends_with_a_next_action() {
        for stop in [Stop::Break, Stop::UnhandledSvc, Stop::Halt, Stop::Fault, Stop::Budget] {
            let text = next_steps(stop, "app.nro", "tier3");
            assert!(text.contains("ounce-core") && text.is_ascii(), "{stop:?}");
        }
        assert!(next_steps(Stop::Halt, "a.nro", "tier4").contains("--cpu=tier4"));
        assert!(next_steps(Stop::Halt, "a.nro", "tier4").contains(ISSUE_FORM));
        assert!(next_steps(Stop::Budget, "a.nro", "tier3").contains("OUNCE_NRO_BUDGET"));
        assert!(!next_steps(Stop::Budget, "a.nro", "tier3").contains(ISSUE_FORM), "a budget stop is not a bug");
    }

    #[test]
    fn missing_file_message_keeps_the_phrase_scripts_match_on() {
        assert!(file_not_found("/x.nro").contains("Failed to read file '/x.nro'"));
    }
}
