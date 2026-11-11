//! Lector de JSON mínimo, sin memoria dinámica: alcanza para los mensajes de
//! Stratum (objetos, listas, textos sin escapes raros, números, `true`,
//! `false` y `null`). Cada [`Json`] es un pedazo del texto original.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Json<'a>(&'a str);

impl<'a> Json<'a> {
    /// El valor entero de `text` (sin espacios alrededor).
    pub fn parse(text: &'a str) -> Option<Json<'a>> {
        let start = skip_ws(text, 0);
        let end = skip_value(text, start)?;
        (skip_ws(text, end) == text.len()).then(|| Json(&text[start..end]))
    }

    pub fn raw(&self) -> &'a str {
        self.0
    }

    pub fn is_null(&self) -> bool {
        self.0 == "null"
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self.0 {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        self.0.parse().ok()
    }

    pub fn as_u64(&self) -> Option<u64> {
        self.0.parse().ok()
    }

    /// Texto sin las comillas. Los escapes quedan tal cual (Stratum manda hex).
    pub fn as_str(&self) -> Option<&'a str> {
        self.0.strip_prefix('"')?.strip_suffix('"')
    }

    /// Valores de una lista.
    pub fn items(&self) -> Items<'a> {
        let inner = self
            .0
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or("");
        Items { s: inner, pos: 0 }
    }

    pub fn at(&self, i: usize) -> Option<Json<'a>> {
        self.items().nth(i)
    }

    /// Miembro `key` de un objeto.
    pub fn get(&self, key: &str) -> Option<Json<'a>> {
        let s = self.0.strip_prefix('{')?.strip_suffix('}')?;
        let mut pos = 0;
        loop {
            pos = skip_ws(s, pos);
            if pos >= s.len() {
                return None;
            }
            let key_end = skip_value(s, pos)?;
            let k = s[pos..key_end].strip_prefix('"')?.strip_suffix('"')?;
            pos = skip_ws(s, key_end);
            if s.as_bytes().get(pos) != Some(&b':') {
                return None;
            }
            pos = skip_ws(s, pos + 1);
            let end = skip_value(s, pos)?;
            if k == key {
                return Some(Json(&s[pos..end]));
            }
            pos = skip_ws(s, end);
            if s.as_bytes().get(pos) == Some(&b',') {
                pos += 1;
            }
        }
    }
}

pub struct Items<'a> {
    s: &'a str,
    pos: usize,
}

impl<'a> Iterator for Items<'a> {
    type Item = Json<'a>;

    fn next(&mut self) -> Option<Json<'a>> {
        let start = skip_ws(self.s, self.pos);
        if start >= self.s.len() {
            return None;
        }
        let end = skip_value(self.s, start)?;
        let mut pos = skip_ws(self.s, end);
        if self.s.as_bytes().get(pos) == Some(&b',') {
            pos += 1;
        }
        self.pos = pos;
        Some(Json(&self.s[start..end]))
    }
}

fn skip_ws(s: &str, mut pos: usize) -> usize {
    let b = s.as_bytes();
    while pos < b.len() && b[pos].is_ascii_whitespace() {
        pos += 1;
    }
    pos
}

/// Dónde termina el valor que empieza en `pos`.
fn skip_value(s: &str, pos: usize) -> Option<usize> {
    let b = s.as_bytes();
    match *b.get(pos)? {
        b'"' => {
            let mut i = pos + 1;
            while i < b.len() {
                match b[i] {
                    b'\\' => i += 2,
                    b'"' => return Some(i + 1),
                    _ => i += 1,
                }
            }
            None
        }
        b'[' | b'{' => {
            let mut depth = 0usize;
            let mut i = pos;
            while i < b.len() {
                match b[i] {
                    b'"' => {
                        i = skip_value(s, i)?;
                        continue;
                    }
                    b'[' | b'{' => depth += 1,
                    b']' | b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i + 1);
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            None
        }
        _ => {
            let mut i = pos;
            while i < b.len() && !matches!(b[i], b',' | b']' | b'}') && !b[i].is_ascii_whitespace()
            {
                i += 1;
            }
            (i > pos).then_some(i)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_stratum_notify() {
        let line = r#"{"id":null,"method":"mining.notify","params":["1a2b","00ff",  "c1","c2",["aa","bb"],"20000000","1703a8c1","66f0a1b2",true]}"#;
        let j = Json::parse(line).unwrap();
        assert!(j.get("id").unwrap().is_null());
        assert_eq!(j.get("method").unwrap().as_str(), Some("mining.notify"));
        let p = j.get("params").unwrap();
        assert_eq!(p.at(0).unwrap().as_str(), Some("1a2b"));
        let branches: Vec<_> = p
            .at(4)
            .unwrap()
            .items()
            .map(|b| b.as_str().unwrap())
            .collect();
        assert_eq!(branches, ["aa", "bb"]);
        assert_eq!(p.at(8).unwrap().as_bool(), Some(true));
        assert_eq!(p.items().count(), 9);
    }

    #[test]
    fn reads_a_subscribe_result() {
        let line = r#"{"id":1,"result":[[["mining.set_difficulty","x"],["mining.notify","y"]],"0a1b2c3d",4],"error":null}"#;
        let j = Json::parse(line).unwrap();
        assert_eq!(j.get("id").unwrap().as_u64(), Some(1));
        let r = j.get("result").unwrap();
        assert_eq!(r.at(1).unwrap().as_str(), Some("0a1b2c3d"));
        assert_eq!(r.at(2).unwrap().as_u64(), Some(4));
        assert!(j.get("error").unwrap().is_null());
    }

    #[test]
    fn numbers_strings_and_garbage() {
        let j = Json::parse(r#"{"params":[0.0001],"s":"a\"b,]"}"#).unwrap();
        assert_eq!(
            j.get("params").unwrap().at(0).unwrap().as_f64(),
            Some(0.0001)
        );
        assert_eq!(j.get("s").unwrap().raw(), r#""a\"b,]""#);
        assert!(j.get("nada").is_none());
        assert!(Json::parse(r#"{"a":[1,2"#).is_none());
        assert!(Json::parse("").is_none());
    }
}
