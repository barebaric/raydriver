//! rpascript parsing.
//!
//! Counterpart of ``ruida_pa.rpascript.interpreter`` (ScriptParser).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::ruida::protocols::ruida::ruida_protocol::{
    CommandDef, COMMANDS, MEMORY,
};
use crate::ruida::rpascript::encoding::CodecError;

pub struct Command {
    pub mnemonic: String,
    pub params: Vec<String>,
}

static COMMAND_INDEX: OnceLock<HashMap<&'static str, &'static CommandDef>> =
    OnceLock::new();
static MEMORY_INDEX: OnceLock<HashMap<&'static str, u16>> = OnceLock::new();

/// Looks up a command by mnemonic.
pub fn lookup(mnemonic: &str) -> Option<&'static CommandDef> {
    COMMAND_INDEX
        .get_or_init(|| {
            COMMANDS
                .iter()
                .map(|entry| (entry.mnemonic, entry))
                .collect()
        })
        .get(mnemonic)
        .copied()
}

/// Looks up a memory address mnemonic.
pub fn memory_address(mnemonic: &str) -> Option<u16> {
    MEMORY_INDEX
        .get_or_init(|| MEMORY.iter().copied().collect())
        .get(mnemonic)
        .copied()
}

pub fn parse_script(lines: &[String]) -> Result<Vec<Command>, CodecError> {
    let mut commands = Vec::new();
    let mut in_block_comment = false;
    for line in lines {
        let (text, still_open) = strip_block_comment(line, in_block_comment);
        in_block_comment = still_open;
        let text = strip_line_comment(&text);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let mut tokens = text.split_whitespace();
        let Some(mnemonic) = tokens.next() else {
            continue;
        };
        let lower = mnemonic.to_ascii_lowercase();
        if matches!(lower.as_str(), "session" | "server" | "new_packet") {
            continue;
        }
        let mut params = Vec::new();
        for token in tokens {
            if token.starts_with('=') {
                break;
            }
            params.push(token.to_string());
        }
        commands.push(Command {
            mnemonic: mnemonic.to_string(),
            params,
        });
    }
    Ok(commands)
}

fn strip_block_comment(line: &str, mut open: bool) -> (String, bool) {
    if !open && !line.contains("\"\"\"") {
        return (line.to_string(), false);
    }
    let mut out = String::new();
    let mut rest = line;
    loop {
        if open {
            match rest.find("\"\"\"") {
                Some(end) => {
                    open = false;
                    rest = &rest[end + 3..];
                }
                None => break,
            }
        } else {
            match rest.find("\"\"\"") {
                Some(start) => {
                    out.push_str(&rest[..start]);
                    open = true;
                    rest = &rest[start + 3..];
                }
                None => {
                    out.push_str(rest);
                    break;
                }
            }
        }
    }
    (out, open)
}

fn strip_line_comment(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'#') {
            out.push('#');
            chars.next();
        } else if ch == '#' {
            break;
        } else {
            out.push(ch);
        }
    }
    out
}
