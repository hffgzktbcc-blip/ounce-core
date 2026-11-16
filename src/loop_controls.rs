//! Day-1 run-loop controls: `--max-frames N` and `--press BUTTON@FRAME`.

use crate::hid::controller::NpadButton;
use std::sync::OnceLock;

/// How many presented frames an injected press stays held.
pub const PRESS_HOLD_FRAMES: usize = 3;

/// Button names accepted by `--press` (case-insensitive).
pub const BUTTON_NAMES: &[(&str, NpadButton)] = &[
    ("A", NpadButton::A),
    ("B", NpadButton::B),
    ("X", NpadButton::X),
    ("Y", NpadButton::Y),
    ("LSTICK", NpadButton::LStick),
    ("RSTICK", NpadButton::RStick),
    ("L", NpadButton::L),
    ("R", NpadButton::R),
    ("ZL", NpadButton::ZL),
    ("ZR", NpadButton::ZR),
    ("PLUS", NpadButton::Plus),
    ("MINUS", NpadButton::Minus),
    ("DLEFT", NpadButton::DLeft),
    ("DUP", NpadButton::DUp),
    ("DRIGHT", NpadButton::DRight),
    ("DDOWN", NpadButton::DDown),
];

/// One scheduled press: `button` goes down at presented frame `frame`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Press {
    pub button: NpadButton,
    pub frame: usize,
}

pub fn button_list() -> String {
    BUTTON_NAMES.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
}

/// Parses `BUTTON@FRAME`, e.g. `PLUS@60`.
pub fn parse_press(spec: &str) -> Result<Press, String> {
    let (name, frame) = spec
        .split_once('@')
        .ok_or_else(|| format!("--press expects BUTTON@FRAME (e.g. PLUS@60), got '{spec}'"))?;
    let button = BUTTON_NAMES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, b)| *b)
        .ok_or_else(|| format!("unknown button '{name}' (valid: {})", button_list()))?;
    let frame = frame.parse::<usize>().map_err(|_| format!("invalid frame '{frame}' in --press '{spec}'"))?;
    Ok(Press { button, frame })
}

/// Parses the `--max-frames` value; N must be at least 1.
pub fn parse_max_frames(value: &str) -> Result<usize, String> {
    match value.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(format!("--max-frames expects a positive integer, got '{value}'")),
    }
}

/// Button bits held while `frame` is presented, given the schedule.
pub fn held_mask(presses: &[Press], frame: usize) -> u64 {
    presses
        .iter()
        .filter(|p| frame >= p.frame && frame < p.frame + PRESS_HOLD_FRAMES)
        .fold(0, |m, p| m | p.button as u64)
}

static MAX_FRAMES: OnceLock<usize> = OnceLock::new();
static PRESSES: OnceLock<Vec<Press>> = OnceLock::new();

pub fn configure(max_frames: Option<usize>, presses: Vec<Press>) {
    if let Some(n) = max_frames {
        let _ = MAX_FRAMES.set(n);
    }
    let _ = PRESSES.set(presses);
}

/// Frame limit from `--max-frames` (0 = unlimited).
pub fn max_frames() -> usize {
    MAX_FRAMES.get().copied().unwrap_or(0)
}

pub fn presses() -> &'static [Press] {
    PRESSES.get().map(Vec::as_slice).unwrap_or(&[])
}

/// True when either flag was given (headless runs then need the HID path).
pub fn active() -> bool {
    max_frames() > 0 || !presses().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_press() {
        assert_eq!(parse_press("PLUS@60"), Ok(Press { button: NpadButton::Plus, frame: 60 }));
        assert_eq!(parse_press("dup@0").unwrap().button, NpadButton::DUp);
    }

    #[test]
    fn rejects_bad_press() {
        assert!(parse_press("PLUS").is_err());
        assert!(parse_press("HOME@3").unwrap_err().contains("valid:"));
        assert!(parse_press("A@x").is_err());
        assert!(parse_press("A@-1").is_err());
    }

    #[test]
    fn max_frames_validation() {
        assert_eq!(parse_max_frames("5"), Ok(5));
        assert!(parse_max_frames("0").is_err());
        assert!(parse_max_frames("abc").is_err());
    }

    #[test]
    fn hold_window() {
        let p = [parse_press("PLUS@60").unwrap(), parse_press("A@61").unwrap()];
        assert_eq!(held_mask(&p, 59), 0);
        assert_eq!(held_mask(&p, 60), NpadButton::Plus as u64);
        assert_eq!(held_mask(&p, 61), NpadButton::Plus as u64 | NpadButton::A as u64);
        assert_eq!(held_mask(&p, 60 + PRESS_HOLD_FRAMES), NpadButton::A as u64);
        assert_eq!(held_mask(&p, 100), 0);
    }
}
