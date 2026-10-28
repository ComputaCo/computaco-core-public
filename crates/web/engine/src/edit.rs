//! The editing model a text control shares between a page with script and one
//! without: a caret and an anchor, in characters of the control's value. A selection
//! is the span between them; the anchor stays where Shift was first held and the
//! caret moves, as in the editors on the desktop (`cw_applications`' code buffer) and
//! in Chromium's `<input>` and `<textarea>`.
//!
//! The rules follow Chromium on Linux:
//! - ArrowLeft/ArrowRight move one character; with a selection and no Shift they
//!   collapse it to its start or end instead of moving.
//! - Ctrl+ArrowLeft/Right move by word; Home/End go to the line's start and end (the
//!   whole value in an `<input>`), Ctrl+Home/End to the value's.
//! - ArrowUp/ArrowDown go to the start and end in an `<input>`, and a line up or down
//!   at the same column in a `<textarea>`.
//! - Shift with any of these extends the selection from the anchor; Ctrl+A selects all.
//! - Typing replaces the selection; Backspace and Delete delete it, or else one
//!   character (Ctrl: one word) before or after the caret.

/// The caret and the anchor, in characters. Equal when nothing is selected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub cursor: usize,
}

impl Selection {
    pub fn caret(at: usize) -> Selection {
        Selection {
            anchor: at,
            cursor: at,
        }
    }
    /// `(start, end)` with `start <= end`.
    pub fn range(&self) -> (usize, usize) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor))
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.cursor
    }
    /// Both ends clamped to a value of `len` characters.
    pub fn clamp(self, len: usize) -> Selection {
        Selection {
            anchor: self.anchor.min(len),
            cursor: self.cursor.min(len),
        }
    }
    /// Everything in a value of `len` characters, the caret at the end.
    pub fn all(len: usize) -> Selection {
        Selection {
            anchor: 0,
            cursor: len,
        }
    }
}

/// Keyboard modifiers an edit depends on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
}

/// Whether `key` (a DOM key name) moves the caret.
pub fn is_motion(key: &str) -> bool {
    matches!(
        key,
        "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" | "Home" | "End"
    )
}

/// Where a motion key leaves the selection in `value`; `None` when `key` is not one.
pub fn motion(
    value: &str,
    sel: Selection,
    key: &str,
    mods: Mods,
    multiline: bool,
) -> Option<Selection> {
    if !is_motion(key) {
        return None;
    }
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    let sel = sel.clamp(len);
    let (start, end) = sel.range();
    // Without Shift, a selection collapses: Left/Right to its edge, the rest move
    // from the caret.
    if !mods.shift && !sel.is_empty() && !mods.ctrl {
        match key {
            "ArrowLeft" => return Some(Selection::caret(start)),
            "ArrowRight" => return Some(Selection::caret(end)),
            _ => {}
        }
    }
    let at = sel.cursor;
    let to = match key {
        "ArrowLeft" if mods.ctrl => word_left(&chars, at),
        "ArrowRight" if mods.ctrl => word_right(&chars, at),
        "ArrowLeft" => at.saturating_sub(1),
        "ArrowRight" => (at + 1).min(len),
        "Home" if mods.ctrl || !multiline => 0,
        "End" if mods.ctrl || !multiline => len,
        "Home" => line_start(&chars, at),
        "End" => line_end(&chars, at),
        "ArrowUp" if !multiline => 0,
        "ArrowDown" if !multiline => len,
        "ArrowUp" => {
            let ls = line_start(&chars, at);
            if ls == 0 {
                0
            } else {
                let prev = line_start(&chars, ls - 1);
                (prev + (at - ls)).min(ls - 1)
            }
        }
        _ => {
            let le = line_end(&chars, at);
            if le == len {
                len
            } else {
                let col = at - line_start(&chars, at);
                let next = le + 1;
                (next + col).min(line_end(&chars, next))
            }
        }
    };
    Some(if mods.shift {
        Selection {
            anchor: sel.anchor,
            cursor: to,
        }
    } else {
        Selection::caret(to)
    })
}

/// `text` typed over the selection: the new value and the caret after it. `max` is
/// the control's `maxlength`; text that would overflow it is cut to fit.
pub fn insert(value: &str, sel: Selection, text: &str, max: Option<usize>) -> (String, usize) {
    let chars: Vec<char> = value.chars().collect();
    let (s, e) = sel.clamp(chars.len()).range();
    let mut ins: Vec<char> = text.chars().collect();
    if let Some(max) = max {
        let room = max.saturating_sub(chars.len() - (e - s));
        ins.truncate(room);
    }
    let mut out: String = chars[..s].iter().collect();
    out.extend(ins.iter());
    out.extend(chars[e..].iter());
    (out, s + ins.len())
}

/// Backspace (`forward` false) or Delete: the selection if there is one, else a
/// character (a word with Ctrl) beside the caret. `None` when there is nothing to
/// delete.
pub fn delete(value: &str, sel: Selection, forward: bool, word: bool) -> Option<(String, usize)> {
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    let (mut s, mut e) = sel.clamp(len).range();
    if s == e {
        if forward {
            if e >= len {
                return None;
            }
            e = if word { word_right(&chars, e) } else { e + 1 };
        } else {
            if s == 0 {
                return None;
            }
            s = if word { word_left(&chars, s) } else { s - 1 };
        }
    }
    let mut out: String = chars[..s].iter().collect();
    out.extend(chars[e..].iter());
    Some((out, s))
}

/// The selected text.
pub fn selected(value: &str, sel: Selection) -> String {
    let (s, e) = sel.range();
    value.chars().skip(s).take(e.saturating_sub(s)).collect()
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn word_left(chars: &[char], at: usize) -> usize {
    let mut i = at.min(chars.len());
    while i > 0 && !is_word(chars[i - 1]) {
        i -= 1;
    }
    while i > 0 && is_word(chars[i - 1]) {
        i -= 1;
    }
    i
}

fn word_right(chars: &[char], at: usize) -> usize {
    let mut i = at.min(chars.len());
    while i < chars.len() && !is_word(chars[i]) {
        i += 1;
    }
    while i < chars.len() && is_word(chars[i]) {
        i += 1;
    }
    i
}

fn line_start(chars: &[char], at: usize) -> usize {
    let mut i = at.min(chars.len());
    while i > 0 && chars[i - 1] != '\n' {
        i -= 1;
    }
    i
}

fn line_end(chars: &[char], at: usize) -> usize {
    let mut i = at.min(chars.len());
    while i < chars.len() && chars[i] != '\n' {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIFT: Mods = Mods {
        shift: true,
        ctrl: false,
    };
    const NONE: Mods = Mods {
        shift: false,
        ctrl: false,
    };

    #[test]
    fn shift_arrows_grow_a_selection_from_the_anchor_and_plain_arrows_collapse_it() {
        let v = "hello world";
        let s = Selection::caret(5);
        let s = motion(v, s, "ArrowLeft", SHIFT, false).unwrap();
        let s = motion(v, s, "ArrowLeft", SHIFT, false).unwrap();
        assert_eq!(
            s,
            Selection {
                anchor: 5,
                cursor: 3
            }
        );
        assert_eq!(selected(v, s), "lo");
        // Back over the anchor: the selection flips side.
        let s = motion(v, s, "ArrowRight", SHIFT, false).unwrap();
        let s = motion(v, s, "ArrowRight", SHIFT, false).unwrap();
        let s = motion(v, s, "ArrowRight", SHIFT, false).unwrap();
        assert_eq!(selected(v, s), " ");
        assert_eq!(
            motion(v, s, "ArrowLeft", NONE, false),
            Some(Selection::caret(5))
        );
        assert_eq!(
            motion(v, s, "ArrowRight", NONE, false),
            Some(Selection::caret(6))
        );
        let end = motion(v, Selection::caret(2), "End", SHIFT, false).unwrap();
        assert_eq!(selected(v, end), "llo world");
        let word = motion(
            v,
            Selection::caret(11),
            "ArrowLeft",
            Mods {
                shift: true,
                ctrl: true,
            },
            false,
        )
        .unwrap();
        assert_eq!(selected(v, word), "world");
    }

    #[test]
    fn a_textarea_moves_by_line_and_home_end_stay_on_the_line() {
        let v = "one\ntwo two\nx";
        let s = Selection::caret(6); // "tw|o"
        assert_eq!(motion(v, s, "Home", NONE, true), Some(Selection::caret(4)));
        assert_eq!(motion(v, s, "End", NONE, true), Some(Selection::caret(11)));
        assert_eq!(
            motion(v, s, "ArrowUp", NONE, true),
            Some(Selection::caret(2))
        );
        assert_eq!(
            motion(v, s, "ArrowDown", NONE, true),
            Some(Selection::caret(13))
        );
        assert_eq!(
            motion(v, s, "ArrowUp", NONE, false),
            Some(Selection::caret(0))
        );
    }

    #[test]
    fn typing_and_deleting_act_on_the_selection_first() {
        let all = Selection::all(5);
        assert_eq!(insert("hello", all, "X", None), ("X".into(), 1));
        assert_eq!(delete("hello", all, false, false), Some((String::new(), 0)));
        let mid = Selection {
            anchor: 4,
            cursor: 1,
        };
        assert_eq!(delete("hello", mid, true, false), Some(("ho".into(), 1)));
        assert_eq!(delete("hello", Selection::caret(0), false, false), None);
        assert_eq!(
            delete("ab cd", Selection::caret(5), false, true),
            Some(("ab ".into(), 3))
        );
        assert_eq!(
            insert("abc", Selection::caret(1), "XYZ", Some(4)),
            ("aXbc".into(), 2)
        );
    }
}
