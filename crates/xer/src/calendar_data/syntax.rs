//! Bounded parser for the parenthesized calendar tree. No P6 code is used.

/// Maximum nested node depth; real calendar trees are shallow.
pub(super) const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 100_000;

#[derive(Debug)]
pub(super) struct Node<'a> {
    pub name: &'a str,
    pub payload: &'a str,
    pub children: Vec<Self>,
}

pub(super) fn parse(source: &str) -> Result<Node<'_>, &'static str> {
    let trimmed = source.trim();
    // Some exporters quote multiline calendar cells. The raw spelling stays intact.
    let source = if let Some(rest) = trimmed.strip_prefix('"') {
        rest.strip_suffix('"').ok_or("unclosed calendar quote")?
    } else {
        trimmed
    };
    let mut parser = Parser {
        source,
        pos: 0,
        nodes: 0,
    };
    let root = parser.node(0)?;
    parser.space();
    if parser.pos != source.len() {
        return Err("trailing calendar content");
    }
    Ok(root)
}

struct Parser<'a> {
    source: &'a str,
    pos: usize,
    nodes: usize,
}

impl<'a> Parser<'a> {
    fn space(&mut self) {
        // DEL is an observed line-break/indentation placeholder in P6 exports.
        while self
            .source
            .as_bytes()
            .get(self.pos)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == 0x7f)
        {
            self.pos += 1;
        }
    }

    fn byte(&mut self, expected: u8) -> Result<(), &'static str> {
        self.space();
        if self.source.as_bytes().get(self.pos) != Some(&expected) {
            return Err("malformed calendar delimiters");
        }
        self.pos += 1;
        Ok(())
    }

    fn until(&mut self, end: u8) -> Result<&'a str, &'static str> {
        let start = self.pos;
        while let Some(&byte) = self.source.as_bytes().get(self.pos) {
            if byte == end {
                let text = &self.source[start..self.pos];
                self.pos += 1;
                return Ok(text);
            }
            if matches!(byte, b'(' | b')') {
                return Err("unexpected calendar delimiter");
            }
            self.pos += 1;
        }
        Err("truncated calendar node")
    }

    fn node(&mut self, depth: usize) -> Result<Node<'a>, &'static str> {
        if depth >= MAX_DEPTH || self.nodes >= MAX_NODES {
            return Err("calendar tree exceeds parser limits");
        }
        self.nodes += 1;
        self.byte(b'(')?;
        let header = self.until(b'(')?;
        let mut parts = header.trim().split('|');
        let (Some(tag), Some(flags), Some(name)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err("invalid calendar node header");
        };
        if tag != "0" || !flags.is_empty() || parts.next().is_some() {
            return Err("unsupported calendar node header");
        }
        let payload = self.until(b')')?;
        self.byte(b'(')?;
        let mut children = Vec::new();
        loop {
            self.space();
            if self.source.as_bytes().get(self.pos) == Some(&b')') {
                break;
            }
            children.push(self.node(depth + 1)?);
        }
        self.byte(b')')?;
        self.byte(b')')?;
        Ok(Node {
            name: name.trim(),
            payload,
            children,
        })
    }
}
