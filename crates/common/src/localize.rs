//! Localized string tables: `localizedstrings/<language>/*.str`, looked up
//! as `<FILE>_<REFERENCE>` the way a `@` reference in a menu or a
//! localized configstring names them.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::pk3::Pk3Fs;

const LANGUAGE_DIR: &str = "localizedstrings/english/";

#[derive(Default)]
pub struct Localized {
    /// Uppercased `<FILE>_<REFERENCE>` to the English text.
    strings: HashMap<String, String>,
}

impl Localized {
    /// Every `.str` under the English directory; a missing or unreadable file
    /// is skipped, so a stripped install still resolves what it has.
    /// The localized `.str` tables, from every `localized_english_pak*.pk3`
    /// in the search path. These live in `main/` beside the other paks (not
    /// at the install root), and the later ones win, so the whole family is
    /// read in archive order and the last value for a reference stands.
    ///
    /// Taking only `pak1` is a real trap: `mpscript.str` and `menu.str` are
    /// in `pak1`, but later paks carry the *additions* to them, and a
    /// reference missing from whichever single pak you picked falls through
    /// `translate` to its own bare name - which is what put
    /// `MPSCRIPT_KILLCAM` and `MPSCRIPT_PRESS_ACTIVATE_TO_RESPAWN` on the HUD
    /// as raw keys.
    pub fn load(fs: &Pk3Fs) -> Localized {
        let mut l = Localized::default();
        for path in fs.list_prefix(LANGUAGE_DIR) {
            let Some(stem) = path
                .strip_prefix(LANGUAGE_DIR)
                .and_then(|p| p.strip_suffix(".str"))
            else {
                continue;
            };
            if let Some(bytes) = fs.read(&path) {
                l.parse_into(stem, &String::from_utf8_lossy(&bytes));
            }
        }
        l
    }

    pub fn parse_into(&mut self, file_stem: &str, text: &str) {
        let prefix = file_stem.to_ascii_uppercase();
        let mut reference: Option<String> = None;
        for line in text.lines() {
            let line = line.trim();
            if let Some(name) = line.strip_prefix("REFERENCE") {
                reference = Some(name.trim().to_ascii_uppercase());
            } else if let Some(rest) = line.strip_prefix("LANG_ENGLISH") {
                if let (Some(name), Some(value)) = (reference.take(), quoted(rest)) {
                    self.strings.insert(format!("{prefix}_{name}"), value);
                }
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.strings
            .get(&key.to_ascii_uppercase())
            .map(String::as_str)
    }

    /// `@KEY` to its text, or the bare key when the table lacks it; any other
    /// text passes through. A `{+command}` token in the result is replaced with
    /// the key bound to it, the way the retail client does at draw time.
    pub fn translate(&self, text: &str) -> String {
        // A reference with no table entry still gets its binds resolved, so
        // `{+activate}` never leaks into a HUD line even untranslated.
        substitute_binds_owned(self.resolve_or_strip(text).as_ref(), &|_| None)
    }

    /// [`Self::translate`] with the player's key bindings, for HUD text that
    /// names a key to press.
    ///
    /// Retail ships these strings with the bind command in braces rather than
    /// a key name, and the client substitutes the player's own binding when it
    /// draws them. Read out of `localizedstrings/english/mpscript.str`:
    ///
    /// ```text
    /// PRESS_ACTIVATE_TO_RESPAWN  "Press [{+activate}] to respawn"
    /// PRESS_ACTIVATE_TO_SKIP     "Press [{+activate}] to skip"
    /// ```
    ///
    /// Without the substitution the HUD shows the literal `[{+activate}]`,
    /// which is what the respawn and killcam-skip prompts both did. Passing
    /// `None` for an unbound command leaves the token in place rather than
    /// printing nothing, so an unbound action stays visible as a name.
    pub fn translate_binds(&self, text: &str, bind_of: impl Fn(&str) -> Option<String>) -> String {
        substitute_binds_owned(self.resolve_or_strip(text).as_ref(), &bind_of)
    }

    /// The table value `text` names, or `None` when it is not a reference.
    ///
    /// A reference is `@KEY` in script source, but *bare* on the wire:
    /// `setText` configstrings the name with no prefix (see
    /// `crates/server/src/game/builtins/hud.rs`). Both forms are accepted, and
    /// a bare string only counts when the table actually has it, so literal
    /// HUD text passes through untouched.
    ///
    /// Both [`Self::translate`] and [`Self::translate_binds`] go through here.
    /// They cannot diverge: when they did, `translate` resolved the reference
    /// and `translate_binds` did not, so the respawn prompt read
    /// `Press [{+activate}] to respawn` while every other localized element
    /// read as its own key.
    fn resolve(&self, text: &str) -> Option<&str> {
        match text.strip_prefix('@') {
            Some(key) => self.get(key),
            None => self.get(text),
        }
    }

    /// What to draw for `text`: the string it names, or - for a reference the
    /// table lacks - the reference *without* its `@`, which is what retail
    /// shows for a missing string. Owns the strip so the borrow outlives the
    /// `Cow` the caller returns.
    fn resolve_or_strip<'a>(&'a self, text: &'a str) -> Cow<'a, str> {
        if let Some(value) = self.resolve(text) {
            return Cow::Borrowed(value);
        }
        match text.strip_prefix('@') {
            Some(key) => Cow::Owned(key.to_string()),
            None => Cow::Borrowed(text),
        }
    }
}

/// [`substitute_binds`] as an owned `String`, for the callers that hand it a
/// value built inside the call and cannot return a borrow of it.
fn substitute_binds_owned(text: &str, bind_of: &impl Fn(&str) -> Option<String>) -> String {
    substitute_binds(text, bind_of).into_owned()
}

/// Replaces every `{+command}` in `text` with the key `bind_of` names.
///
/// Retail writes these as a braced command so one string serves every binding
/// scheme; a bind table can be edited, so the text is resolved at draw time
/// rather than baked. An unknown command is left verbatim, which keeps the
/// prompt readable instead of blanking it.
fn substitute_binds<'a>(text: &'a str, bind_of: &impl Fn(&str) -> Option<String>) -> Cow<'a, str> {
    if !text.contains("{+") {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("{+") {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 2..];
        match tail.find('}') {
            Some(end) => {
                let command = &tail[..end];
                match bind_of(command) {
                    Some(key) => out.push_str(&key),
                    None => out.push_str(&rest[at..at + 2 + end + 1]),
                }
                rest = &tail[end + 1..];
            }
            None => {
                out.push_str(&rest[at..]);
                return Cow::Owned(out);
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// The text between the first and the last `"`, with `\n` and `\"` unescaped.
fn quoted(s: &str) -> Option<String> {
    let start = s.find('"')?;
    let end = s.rfind('"')?;
    (end > start).then(|| s[start + 1..end].replace("\\n", "\n").replace("\\\"", "\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "// comment\r\nVERSION \"1\"\r\nCONFIG \"C:\\x\\y.cfg\"\r\nFILENOTES \"\"\r\n\r\nREFERENCE MAIN_MENU\r\nLANG_ENGLISH \"Main Menu\"\r\n\r\nREFERENCE 1_AMERICAN\r\nLANG_ENGLISH \"1. American\"\r\n";

    #[test]
    fn parses_references_under_the_file_prefix() {
        let mut l = Localized::default();
        l.parse_into("mpmenu", SAMPLE);
        assert_eq!(l.get("MPMENU_1_AMERICAN"), Some("1. American"));
        assert_eq!(l.get("mpmenu_main_menu"), Some("Main Menu"));
        assert_eq!(l.get("MPMENU_NOPE"), None);
    }

    #[test]
    fn translate_resolves_only_an_at_reference() {
        let mut l = Localized::default();
        l.parse_into("mpmenu", SAMPLE);
        assert_eq!(l.translate("@MPMENU_MAIN_MENU"), "Main Menu");
        assert_eq!(l.translate("plain"), "plain");
        // An unknown reference shows its key, as retail does.
        assert_eq!(l.translate("@MPMENU_NOPE"), "MPMENU_NOPE");
    }

    #[test]
    fn stock_tables_carry_the_menu_strings() {
        let Some(fs) = crate::testing::game_fs() else {
            return;
        };
        let l = Localized::load(&fs);
        assert_eq!(l.get("MPMENU_1_AMERICAN"), Some("1. American"));
        assert!(l.get("MPMENU_1_M1A1_CARBINE").is_some());
    }

    /// The prompts the player actually sees must be words, not keys.
    ///
    /// These two are what `dm.gsc` hands to `setText` on death, and both were
    /// drawn as their own reference names because the language pak is at the
    /// install root rather than in `main/`, so a `Pk3Fs` mounted on `main/`
    /// found no `.str` files at all. A regression here reads as "the HUD is
    /// broken" with nothing in the log to explain it.
    #[test]
    #[ignore = "needs a retail install; set COD_DIR"]
    fn the_retail_prompts_resolve_to_words_not_keys() {
        let game = std::env::var("COD_DIR").expect("COD_DIR");
        let fs = Pk3Fs::open(&std::path::Path::new(&game).join("main")).unwrap();
        let l = Localized::load(&fs);
        // Bare, as the wire carries it: `setText` configstrings the name
        // with no `@`.
        for key in ["MPSCRIPT_PRESS_ACTIVATE_TO_RESPAWN", "MPSCRIPT_KILLCAM"] {
            let out = l.translate(key);
            assert_ne!(
                out, key,
                "{key} fell through to its own name: no .str tables loaded"
            );
            assert!(!out.starts_with("MPSCRIPT_"), "{key} -> {out}");
            println!("{key} -> {out}");
        }
        // The respawn prompt is the one that carries a bind token, and this
        // is the string the player actually sees: the bare wire form run
        // through the bind substitution. It must read as a key, never as
        // `{+activate}` and never as the reference name.
        let binds = |c: &str| match c {
            "activate" => Some("F".to_string()),
            _ => None,
        };
        let drawn = l.translate_binds("MPSCRIPT_PRESS_ACTIVATE_TO_RESPAWN", binds);
        println!("drawn: {drawn}");
        assert_eq!(drawn, "Press [F] to respawn");
        assert!(!drawn.contains("MPSCRIPT_"), "{drawn}");
        assert!(!drawn.contains("{+"), "{drawn}");
    }
}
