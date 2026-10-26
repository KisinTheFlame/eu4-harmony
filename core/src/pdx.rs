//! Just enough understanding of Paradox script text to splice GUI elements into
//! a `windowType` block: a tokenizer that honours quoted strings and `#`
//! comments, a block finder, and the rewrite of a fragment's element names and
//! positions.
use crate::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Ident,
    Str,
    Eq,
    Open,
    Close,
}

#[derive(Clone, Copy, Debug)]
pub struct Token {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
}

pub fn tokenize(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' => {
                let start = i;
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += 1;
                }
                i = (i + 1).min(bytes.len());
                tokens.push(Token { kind: Kind::Str, start, end: i });
            }
            b'=' => {
                tokens.push(Token { kind: Kind::Eq, start: i, end: i + 1 });
                i += 1;
            }
            b'{' => {
                tokens.push(Token { kind: Kind::Open, start: i, end: i + 1 });
                i += 1;
            }
            b'}' => {
                tokens.push(Token { kind: Kind::Close, start: i, end: i + 1 });
                i += 1;
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                let start = i;
                while i < bytes.len() && !matches!(bytes[i], b'#' | b'"' | b'=' | b'{' | b'}') && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                tokens.push(Token { kind: Kind::Ident, start, end: i });
            }
        }
    }
    tokens
}

fn text_of<'a>(text: &'a str, t: &Token) -> &'a str {
    &text[t.start..t.end]
}

fn unquote(s: &str) -> &str {
    s.trim_matches('"')
}

/// A `key = { ... }` block: token indices of the key and of both braces.
#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub key: usize,
    pub open: usize,
    pub close: usize,
}

/// Index of the `}` matching the `{` at `open`.
fn matching_close(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, t) in tokens.iter().enumerate().skip(open) {
        match t.kind {
            Kind::Open => depth += 1,
            Kind::Close => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Blocks whose opening brace is a direct child of the range `(from, to)`.
pub fn child_blocks(tokens: &[Token], from: usize, to: usize) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut i = from;
    while i + 2 < to {
        if tokens[i].kind == Kind::Ident && tokens[i + 1].kind == Kind::Eq && tokens[i + 2].kind == Kind::Open {
            if let Some(close) = matching_close(tokens, i + 2) {
                blocks.push(Block { key: i, open: i + 2, close });
                i = close + 1;
                continue;
            }
        }
        i += 1;
    }
    blocks
}

/// Value of `key = <value>` directly inside the block, as text.
fn direct_value<'a>(text: &'a str, tokens: &[Token], block: Block, key: &str) -> Option<&'a str> {
    let mut i = block.open + 1;
    while i + 2 < block.close {
        let t = tokens[i];
        if t.kind == Kind::Ident && text_of(text, &t).eq_ignore_ascii_case(key) && tokens[i + 1].kind == Kind::Eq {
            let v = tokens[i + 2];
            return match v.kind {
                Kind::Ident | Kind::Str => Some(unquote(text_of(text, &v))),
                _ => None,
            };
        }
        if t.kind == Kind::Open {
            i = matching_close(tokens, i).map_or(block.close, |c| c + 1);
            continue;
        }
        i += 1;
    }
    None
}

/// Finds the `windowType = { name = "<window>" ... }` block anywhere in the file.
pub fn find_window(text: &str, tokens: &[Token], window: &str) -> Option<Block> {
    let mut stack = vec![(0usize, tokens.len())];
    while let Some((from, to)) = stack.pop() {
        for block in child_blocks(tokens, from, to) {
            let key = text_of(text, &tokens[block.key]);
            if key.eq_ignore_ascii_case("windowType") && direct_value(text, tokens, block, "name") == Some(window) {
                return Some(block);
            }
            stack.push((block.open + 1, block.close));
        }
    }
    None
}

/// An element of a fragment after rendering: its block key (for example
/// `guiButtonType`) and the name the mod gave it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    pub key: String,
    pub name: String,
}

pub struct Rendered {
    pub text: String,
    pub elements: Vec<Element>,
}

/// `x` and `y` of a `position = { x = .. y = .. }` block; missing values are 0.
fn read_position(text: &str, tokens: &[Token], block: Block) -> Result<(i32, i32)> {
    let mut xy = (0, 0);
    let mut i = block.open + 1;
    while i + 2 < block.close + 1 {
        let key = text_of(text, &tokens[i]);
        if tokens[i].kind == Kind::Ident && tokens[i + 1].kind == Kind::Eq && tokens[i + 2].kind == Kind::Ident {
            let value: i32 = text_of(text, &tokens[i + 2]).parse().map_err(|_| format!("bad position value in `{key}`"))?;
            if key.eq_ignore_ascii_case("x") {
                xy.0 = value;
            } else if key.eq_ignore_ascii_case("y") {
                xy.1 = value;
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    Ok(xy)
}

/// Rewrites one top-level element of a fragment: the element's `name` gets
/// the mod prefix, and with a slot its `position` becomes slot + own offset.
fn rewrite_element(text: &str, tokens: &[Token], block: Block, prefix: &str, slot: Option<(i32, i32)>) -> Result<(String, String)> {
    let name_token = (block.open + 1..block.close)
        .find(|&i| {
            tokens[i].kind == Kind::Ident
                && text_of(text, &tokens[i]).eq_ignore_ascii_case("name")
                && tokens.get(i + 1).map(|t| t.kind) == Some(Kind::Eq)
                && tokens.get(i + 2).map(|t| t.kind) == Some(Kind::Str)
        })
        .map(|i| tokens[i + 2])
        .ok_or_else(|| format!("element `{}` has no name", text_of(text, &tokens[block.key])))?;
    let name = unquote(text_of(text, &name_token)).to_string();
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(format!("element name {name:?} must be ASCII letters, digits or underscores"));
    }
    // Edits as (start, end, replacement), applied back to front.
    let mut edits: Vec<(usize, usize, String)> = vec![(name_token.start, name_token.end, format!("\"{prefix}{name}\""))];
    if let Some((sx, sy)) = slot {
        let existing = child_blocks(tokens, block.open + 1, block.close)
            .into_iter()
            .find(|b| text_of(text, &tokens[b.key]).eq_ignore_ascii_case("position"));
        let (dx, dy) = match existing {
            Some(b) => read_position(text, tokens, b)?,
            None => (0, 0),
        };
        let position = format!("position = {{ x = {} y = {} }}", sx + dx, sy + dy);
        match existing {
            Some(b) => edits.push((tokens[b.key].start, tokens[b.close].end, position)),
            None => edits.push((tokens[block.open].end, tokens[block.open].end, format!("\n\t{position}"))),
        }
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = text[tokens[block.key].start..tokens[block.close].end].to_string();
    let base = tokens[block.key].start;
    for (start, end, replacement) in edits {
        out.replace_range(start - base..end - base, &replacement);
    }
    Ok((out, name))
}

/// Renders a fragment's top-level elements for splicing. With `slot`, every
/// element is placed relative to that button-area slot.
pub fn render_fragment(fragment: &str, prefix: &str, slot: Option<(i32, i32)>) -> Result<Rendered> {
    let tokens = tokenize(fragment);
    let blocks = child_blocks(&tokens, 0, tokens.len());
    if blocks.is_empty() {
        return Err("fragment has no elements".into());
    }
    let mut out = Rendered { text: String::new(), elements: Vec::new() };
    for block in blocks {
        let (text, name) = rewrite_element(fragment, &tokens, block, prefix, slot)?;
        if out.elements.iter().any(|e| e.name == name) {
            return Err(format!("element name {name:?} is used twice"));
        }
        out.text.push_str("\n\t\t");
        out.text.push_str(&text);
        out.text.push('\n');
        out.elements.push(Element { key: text_of(fragment, &tokens[block.key]).to_string(), name });
    }
    Ok(out)
}

/// Inserts `additions` before the closing brace of the named window.
pub fn splice_window(file: &str, window: &str, additions: &str) -> Result<String> {
    let tokens = tokenize(file);
    let block = find_window(file, &tokens, window).ok_or_else(|| format!("window {window:?} not found"))?;
    let at = tokens[block.close].start;
    let mut out = String::with_capacity(file.len() + additions.len());
    out.push_str(&file[..at]);
    out.push_str(additions);
    out.push_str(&file[at..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"
guiTypes = {
	windowType = {
		name = "other_panel" # has a } in a comment
		windowType = { name = "nested" size = { x = 1 y = 2 } }
	}
	windowType = {
		name = "single_unitpanel"
		backGround=""
		size = { x=400 y=100 }
		guiButtonType = { name = "enable_rebel_button" position = { x=378 y=325 } buttonText = "}" }
	}
	windowType = { name = "after" }
}
"#;

    #[test]
    fn finds_window_by_direct_name_only() {
        let tokens = tokenize(FILE);
        let b = find_window(FILE, &tokens, "single_unitpanel").unwrap();
        assert_eq!(&FILE[tokens[b.key].start..tokens[b.key].end], "windowType");
        assert!(find_window(FILE, &tokens, "nested").is_some());
        assert!(find_window(FILE, &tokens, "missing").is_none());
    }

    #[test]
    fn splice_inserts_before_closing_brace_of_target_only() {
        let out = splice_window(FILE, "single_unitpanel", "\n\t\tADDED\n").unwrap();
        let idx = out.find("ADDED").unwrap();
        assert!(out[..idx].contains("enable_rebel_button"));
        assert!(out[idx..].trim_start_matches("ADDED").trim_start().starts_with('}'));
        assert!(out[idx..].contains("name = \"after\""));
        assert_eq!(out.len(), FILE.len() + "\n\t\tADDED\n".len());
    }

    #[test]
    fn fragment_gets_prefix_and_slot_relative_positions() {
        let fragment = "guiButtonType = {\n\tname = \"hello\"\n\tquadTextureSprite = \"GFX_x\"\n}\niconType = { name = \"glow\" position = { x = -2 y = 3 } }\n";
        let out = render_fragment(fragment, "eu4harmony_hi_", Some((464, 69))).unwrap();
        assert!(out.text.contains("name = \"eu4harmony_hi_hello\""));
        assert!(out.text.contains("position = { x = 464 y = 69 }"));
        assert!(out.text.contains("name = \"eu4harmony_hi_glow\""));
        assert!(out.text.contains("position = { x = 462 y = 72 }"));
        assert!(out.text.contains("quadTextureSprite = \"GFX_x\""));
        assert_eq!(
            out.elements,
            vec![
                Element { key: "guiButtonType".into(), name: "hello".into() },
                Element { key: "iconType".into(), name: "glow".into() }
            ]
        );
    }

    #[test]
    fn fragment_as_written_keeps_position_and_rejects_bad_names() {
        let fragment = "windowType = { name = \"settings\" position = { x = 10 y = 20 } }";
        let out = render_fragment(fragment, "p_", None).unwrap();
        assert!(out.text.contains("name = \"p_settings\"") && out.text.contains("x = 10 y = 20"));
        assert!(render_fragment("guiButtonType = { name = \"a-b\" }", "p_", None).is_err());
        assert!(render_fragment("guiButtonType = { size = { x = 1 y = 1 } }", "p_", None).is_err());
        assert!(render_fragment("iconType = { name = \"a\" } iconType = { name = \"a\" }", "p_", None).is_err());
        assert!(render_fragment("# nothing\n", "p_", None).is_err());
    }
}
