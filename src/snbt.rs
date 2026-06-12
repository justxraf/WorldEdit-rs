use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum SnbtValue {
    Compound(BTreeMap<String, SnbtValue>),
    List(Vec<SnbtValue>),
    String(String),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    Bool(bool),
}

impl SnbtValue {
    pub fn as_compound(&self) -> Option<&BTreeMap<String, SnbtValue>> {
        match self {
            Self::Compound(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[SnbtValue]> {
        match self {
            Self::List(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_bool_loose(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Int(value) => Some(*value != 0),
            Self::Long(value) => Some(*value != 0),
            Self::String(value) if value.eq_ignore_ascii_case("true") => Some(true),
            Self::String(value) if value.eq_ignore_ascii_case("false") => Some(false),
            _ => None,
        }
    }
}

pub fn parse(input: &str) -> Result<SnbtValue, String> {
    let mut parser = Parser::new(input);
    let value = parser.parse_value()?;
    parser.skip_ws();
    if !parser.is_eof() {
        return Err(format!(
            "Unexpected trailing SNBT input '{}'.",
            &parser.input[parser.index..]
        ));
    }
    Ok(value)
}

struct Parser<'a> {
    input: &'a str,
    index: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, index: 0 }
    }

    fn parse_value(&mut self) -> Result<SnbtValue, String> {
        self.skip_ws();
        let Some(ch) = self.peek() else {
            return Err("Unexpected end of SNBT input.".to_string());
        };
        match ch {
            '{' => self.parse_compound(),
            '[' => self.parse_list(),
            '\'' | '"' => Ok(SnbtValue::String(self.parse_quoted_string()?)),
            _ => self.parse_scalar(),
        }
    }

    fn parse_compound(&mut self) -> Result<SnbtValue, String> {
        self.expect('{')?;
        let mut entries = BTreeMap::new();
        loop {
            self.skip_ws();
            if self.take_if('}') {
                break;
            }
            let key = self.parse_key()?;
            self.skip_ws();
            self.expect(':')?;
            let value = self.parse_value()?;
            entries.insert(key, value);
            self.skip_ws();
            if self.take_if('}') {
                break;
            }
            self.expect(',')?;
        }
        Ok(SnbtValue::Compound(entries))
    }

    fn parse_list(&mut self) -> Result<SnbtValue, String> {
        self.expect('[')?;
        let mut values = Vec::new();
        loop {
            self.skip_ws();
            if self.take_if(']') {
                break;
            }
            values.push(self.parse_value()?);
            self.skip_ws();
            if self.take_if(']') {
                break;
            }
            self.expect(',')?;
        }
        Ok(SnbtValue::List(values))
    }

    fn parse_key(&mut self) -> Result<String, String> {
        self.skip_ws();
        match self.peek() {
            Some('\'') | Some('"') => self.parse_quoted_string(),
            _ => self.parse_unquoted_token(),
        }
    }

    fn parse_scalar(&mut self) -> Result<SnbtValue, String> {
        let token = self.parse_unquoted_token()?;
        let lowered = token.to_ascii_lowercase();
        if lowered == "true" {
            return Ok(SnbtValue::Bool(true));
        }
        if lowered == "false" {
            return Ok(SnbtValue::Bool(false));
        }

        if let Some(stripped) = token.strip_suffix(['b', 'B']) {
            let value = stripped
                .parse::<i8>()
                .map_err(|_| format!("Invalid SNBT byte '{token}'."))?;
            return Ok(SnbtValue::Int(i32::from(value)));
        }
        if let Some(stripped) = token.strip_suffix(['l', 'L']) {
            let value = stripped
                .parse::<i64>()
                .map_err(|_| format!("Invalid SNBT long '{token}'."))?;
            return Ok(SnbtValue::Long(value));
        }
        if let Some(stripped) = token.strip_suffix(['f', 'F']) {
            let value = stripped
                .parse::<f32>()
                .map_err(|_| format!("Invalid SNBT float '{token}'."))?;
            return Ok(SnbtValue::Float(value));
        }
        if let Some(stripped) = token.strip_suffix(['d', 'D']) {
            let value = stripped
                .parse::<f64>()
                .map_err(|_| format!("Invalid SNBT double '{token}'."))?;
            return Ok(SnbtValue::Double(value));
        }
        if token.contains('.') || token.contains('e') || token.contains('E') {
            let value = token
                .parse::<f64>()
                .map_err(|_| format!("Invalid SNBT number '{token}'."))?;
            return Ok(SnbtValue::Double(value));
        }
        if let Ok(value) = token.parse::<i32>() {
            return Ok(SnbtValue::Int(value));
        }
        if let Ok(value) = token.parse::<i64>() {
            return Ok(SnbtValue::Long(value));
        }
        Ok(SnbtValue::String(token))
    }

    fn parse_quoted_string(&mut self) -> Result<String, String> {
        let quote = self
            .next()
            .ok_or_else(|| "Unexpected end of SNBT input.".to_string())?;
        let mut value = String::new();
        while let Some(ch) = self.next() {
            if ch == quote {
                return Ok(value);
            }
            if ch == '\\' {
                let escaped = self
                    .next()
                    .ok_or_else(|| "Unfinished escape sequence in SNBT string.".to_string())?;
                value.push(match escaped {
                    '\\' => '\\',
                    '\'' => '\'',
                    '"' => '"',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    other => other,
                });
            } else {
                value.push(ch);
            }
        }
        Err("Unclosed SNBT string literal.".to_string())
    }

    fn parse_unquoted_token(&mut self) -> Result<String, String> {
        let start = self.index;
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() || matches!(ch, ',' | ':' | '}' | ']') {
                break;
            }
            self.next();
        }
        if self.index == start {
            return Err("Expected an SNBT token.".to_string());
        }
        Ok(self.input[start..self.index].to_string())
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.next();
        }
    }

    fn take_if(&mut self, ch: char) -> bool {
        if self.peek() == Some(ch) {
            self.next();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, ch: char) -> Result<(), String> {
        match self.next() {
            Some(actual) if actual == ch => Ok(()),
            Some(actual) => Err(format!("Expected '{ch}' in SNBT, found '{actual}'.")),
            None => Err(format!("Expected '{ch}' in SNBT, found end of input.")),
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.index..].chars().next()
    }

    fn next(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.index += ch.len_utf8();
        Some(ch)
    }

    fn is_eof(&self) -> bool {
        self.index >= self.input.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_compound() {
        let value = parse("{'is_waxed':1}").unwrap();
        let compound = value.as_compound().unwrap();
        assert_eq!(compound.get("is_waxed"), Some(&SnbtValue::Int(1)));
    }

    #[test]
    fn parses_nested_lists_and_quotes() {
        let value = parse("{front_text:{messages:['Hello world','Two']}}").unwrap();
        let compound = value.as_compound().unwrap();
        let front = compound
            .get("front_text")
            .unwrap()
            .as_compound()
            .unwrap()
            .get("messages")
            .unwrap()
            .as_list()
            .unwrap();
        assert_eq!(front[0].as_str(), Some("Hello world"));
        assert_eq!(front[1].as_str(), Some("Two"));
    }
}
