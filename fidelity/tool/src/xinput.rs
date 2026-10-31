//! Delivering stimuli to a virtual display through xdotool, with the procedure's key
//! spelling translated to X keysyms.
use crate::util::{err, Result};
use cw_fidelity::{PointerOp, Stimulus, WindowOp};
use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::time::Duration;

pub struct XInput<'a> {
    pub env: &'a BTreeMap<String, String>,
}

/// `Ctrl+Shift+s` → `ctrl+shift+s`; `ArrowLeft` → `Left`; `Ctrl+\`` → `ctrl+grave`.
pub fn keysym(chord: &str) -> String {
    let parts: Vec<&str> = chord.split('+').collect();
    let mut out = Vec::new();
    // `Ctrl++` is Ctrl and plus.
    let (mods, key) = if chord.ends_with("++") {
        (&parts[..parts.len() - 2], "+")
    } else {
        (&parts[..parts.len() - 1], *parts.last().unwrap_or(&""))
    };
    for m in mods {
        out.push(match m.to_lowercase().as_str() {
            "ctrl" | "control" => "ctrl",
            "alt" | "option" => "alt",
            "shift" => "shift",
            "meta" | "cmd" | "command" | "super" | "win" => "super",
            other => return format!("{other}+{key}"),
        });
    }
    let k = match key {
        "ArrowLeft" => "Left".to_owned(),
        "ArrowRight" => "Right".to_owned(),
        "ArrowUp" => "Up".to_owned(),
        "ArrowDown" => "Down".to_owned(),
        "Enter" => "Return".to_owned(),
        "Escape" | "Esc" => "Escape".to_owned(),
        "Space" | " " => "space".to_owned(),
        "Backspace" => "BackSpace".to_owned(),
        "Delete" => "Delete".to_owned(),
        "Tab" => "Tab".to_owned(),
        "Home" | "End" | "PageUp" | "PageDown" | "Insert" => key.to_owned(),
        "Meta" | "Super" => "super".to_owned(),
        "`" => "grave".to_owned(),
        "-" => "minus".to_owned(),
        "=" => "equal".to_owned(),
        "+" => "plus".to_owned(),
        "," => "comma".to_owned(),
        "." => "period".to_owned(),
        "/" => "slash".to_owned(),
        ";" => "semicolon".to_owned(),
        "'" => "apostrophe".to_owned(),
        "[" => "bracketleft".to_owned(),
        "]" => "bracketright".to_owned(),
        "\\" => "backslash".to_owned(),
        "<" => "less".to_owned(),
        ">" => "greater".to_owned(),
        "?" => "question".to_owned(),
        "!" => "exclam".to_owned(),
        "@" => "at".to_owned(),
        "#" => "numbersign".to_owned(),
        "$" => "dollar".to_owned(),
        "%" => "percent".to_owned(),
        "^" => "asciicircum".to_owned(),
        "&" => "ampersand".to_owned(),
        "*" => "asterisk".to_owned(),
        "(" => "parenleft".to_owned(),
        ")" => "parenright".to_owned(),
        "_" => "underscore".to_owned(),
        "{" => "braceleft".to_owned(),
        "}" => "braceright".to_owned(),
        "|" => "bar".to_owned(),
        "~" => "asciitilde".to_owned(),
        "\"" => "quotedbl".to_owned(),
        ":" => "colon".to_owned(),
        k if k.len() == 1 => k.to_owned(),
        k => k.to_owned(),
    };
    out.push(&k);
    out.join("+")
}

impl XInput<'_> {
    fn xdotool(&self, args: &[&str]) -> Result<()> {
        let mut c = Command::new("xdotool");
        c.args(args)
            .env_clear()
            .envs(self.env)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let out = c.output().map_err(|e| format!("xdotool: {e}"))?;
        if !out.status.success() {
            return err(format!(
                "xdotool {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(())
    }
    pub fn move_to(&self, x: i32, y: i32) -> Result<()> {
        self.xdotool(&["mousemove", "--sync", &x.to_string(), &y.to_string()])
    }
    pub fn deliver(&self, s: &Stimulus) -> Result<()> {
        match s {
            Stimulus::Pointer {
                op,
                x,
                y,
                button,
                modifiers,
                delta_y,
                delta_x,
            } => {
                let mods: Vec<String> = modifiers
                    .iter()
                    .map(|m| keysym(&format!("{m}+x")).trim_end_matches("+x").to_owned())
                    .collect();
                for m in &mods {
                    self.xdotool(&["keydown", m])?;
                }
                let r = (|| {
                    self.move_to(*x, *y)?;
                    std::thread::sleep(Duration::from_millis(40));
                    let b = button.to_string();
                    match op {
                        PointerOp::Move => Ok(()),
                        PointerOp::Down => self.xdotool(&["mousedown", &b]),
                        PointerOp::Up => self.xdotool(&["mouseup", &b]),
                        PointerOp::Click => self.xdotool(&["click", &b]),
                        PointerOp::DoubleClick => {
                            self.xdotool(&["click", "--repeat", "2", "--delay", "80", &b])
                        }
                        PointerOp::Wheel => {
                            let notches = ((delta_y.abs() + 119) / 120).max(1) as usize;
                            let b = if *delta_y > 0 { "5" } else { "4" };
                            for _ in 0..notches {
                                self.xdotool(&["click", b])?;
                            }
                            if *delta_x != 0 {
                                let notches = ((delta_x.abs() + 119) / 120).max(1) as usize;
                                let b = if *delta_x > 0 { "7" } else { "6" };
                                for _ in 0..notches {
                                    self.xdotool(&["click", b])?;
                                }
                            }
                            Ok(())
                        }
                    }
                })();
                for m in mods.iter().rev() {
                    let _ = self.xdotool(&["keyup", m]);
                }
                r
            }
            Stimulus::Drag {
                x,
                y,
                to_x,
                to_y,
                button,
            } => {
                let b = button.to_string();
                self.move_to(*x, *y)?;
                std::thread::sleep(Duration::from_millis(40));
                self.xdotool(&["mousedown", &b])?;
                for i in 1..=6 {
                    let t = i as f32 / 6.0;
                    let mx = *x as f32 + (*to_x - *x) as f32 * t;
                    let my = *y as f32 + (*to_y - *y) as f32 * t;
                    self.move_to(mx.round() as i32, my.round() as i32)?;
                    std::thread::sleep(Duration::from_millis(25));
                }
                self.xdotool(&["mouseup", &b])
            }
            Stimulus::Window { op } => match op {
                WindowOp::Maximize => self.xdotool(&[
                    "getactivewindow",
                    "windowmove",
                    "0",
                    "0",
                    "windowsize",
                    "100%",
                    "100%",
                ]),
                WindowOp::Restore => self.xdotool(&["getactivewindow", "windowsize", "60%", "60%"]),
                WindowOp::Minimize => self.xdotool(&["getactivewindow", "windowminimize"]),
                WindowOp::Close => self.xdotool(&["getactivewindow", "windowclose"]),
            },
            Stimulus::Key { key } => self.xdotool(&["key", "--clearmodifiers", &keysym(key)]),
            Stimulus::Type { text } => {
                let mut first = true;
                for line in text.split('\n') {
                    if !first {
                        self.xdotool(&["key", "Return"])?;
                    }
                    first = false;
                    if !line.is_empty() {
                        self.xdotool(&["type", "--delay", "12", "--", line])?;
                    }
                }
                Ok(())
            }
            Stimulus::Wait { ticks } => {
                std::thread::sleep(Duration::from_millis(*ticks));
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keysyms() {
        assert_eq!(keysym("Ctrl+s"), "ctrl+s");
        assert_eq!(keysym("Ctrl+Shift+S"), "ctrl+shift+S");
        assert_eq!(keysym("ArrowLeft"), "Left");
        assert_eq!(keysym("Ctrl+`"), "ctrl+grave");
        assert_eq!(keysym("Escape"), "Escape");
        assert_eq!(keysym("Ctrl+Alt+Enter"), "ctrl+alt+Return");
        assert_eq!(keysym("Backspace"), "BackSpace");
        assert_eq!(keysym("Ctrl++"), "ctrl+plus");
    }
}
