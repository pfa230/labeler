//! Interpolation token grammar parser and scanner (issue #239, #240).
//!
//! A token is written as `{value-path}` or `{value-path:format-name}`.
//! There are two namespace roots: `vars.<key>` and `sys.<name>` (closed set: `now`).
//! Bare tokens name request fields or parameters.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysValue {
    Now,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source<'a> {
    Bare(&'a str),
    Vars(&'a str),
    Sys(SysValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token<'a> {
    pub source: Source<'a>,
    pub format: Option<&'a str>,
    pub raw: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    /// Token or segment is empty or whitespace-only (e.g. `{}`, `{vars.}`, `{.x}`).
    EmptySegment(String),
    /// Invalid format syntax (e.g. `{:fmt}`, `{x:}`, `{x:a:b}`).
    InvalidFormat(String),
    /// Name contains characters outside `^[a-zA-Z0-9_-]+$` (e.g. `{ id }`, `{my field}`).
    MalformedName(String),
    /// Namespace root is not `vars` or `sys` (e.g. `{datetime.long_date}`, `{a.b}`, `{VARS.x}`).
    UnknownSource { token: String, source: String },
    /// System value is outside the closed set (e.g. `{sys.nwo}`, `{sys.now.long_date}`).
    UnknownSysValue { token: String, value: String },
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::EmptySegment(tok) => {
                write!(f, "template contains '{tok}': empty segment in token")
            }
            TokenError::InvalidFormat(tok) => {
                write!(f, "template contains '{tok}': invalid format in token")
            }
            TokenError::MalformedName(tok) => {
                write!(
                    f,
                    "template contains '{tok}': invalid characters in token name; must match ^[a-zA-Z0-9_-]+$"
                )
            }
            TokenError::UnknownSource { token, source } => {
                if source == "datetime" {
                    if let Some(tail) = token
                        .strip_prefix('{')
                        .and_then(|s| s.strip_suffix('}'))
                        .and_then(|s| s.strip_prefix("datetime."))
                    {
                        write!(
                            f,
                            "template contains '{token}': unknown source 'datetime'; use '{{sys.now:{tail}}}' instead"
                        )
                    } else {
                        write!(f, "template contains '{token}': unknown source 'datetime'")
                    }
                } else {
                    write!(f, "template contains '{token}': unknown source '{source}'")
                }
            }
            TokenError::UnknownSysValue { token, value } => {
                if let Some(tail) = value.strip_prefix("now.") {
                    write!(
                        f,
                        "template contains '{token}': unknown system value '{value}'; use '{{sys.now:{tail}}}' instead"
                    )
                } else {
                    write!(
                        f,
                        "template contains '{token}': unknown system value '{value}' under 'sys'"
                    )
                }
            }
        }
    }
}

impl std::error::Error for TokenError {}

pub(crate) fn is_valid_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Parse a token string (either with outer `{...}` or the inner content).
pub fn parse(raw: &str) -> Result<Token<'_>, TokenError> {
    let (inner, raw_token) = if raw.starts_with('{') && raw.ends_with('}') && raw.len() >= 2 {
        (&raw[1..raw.len() - 1], raw)
    } else {
        (raw, raw)
    };

    if inner.is_empty() || inner.trim().is_empty() {
        return Err(TokenError::EmptySegment(raw_token.to_string()));
    }

    let colon_count = inner.chars().filter(|&c| c == ':').count();
    if colon_count > 1 {
        return Err(TokenError::InvalidFormat(raw_token.to_string()));
    }

    let (val_path, format) = if colon_count == 1 {
        let (vp, fmt) = inner
            .split_once(':')
            .expect("colon_count == 1 guarantees split_once succeeds");
        if vp.is_empty() || vp.trim().is_empty() {
            return Err(TokenError::EmptySegment(raw_token.to_string()));
        }
        if fmt.is_empty() || !is_valid_ident(fmt) {
            return Err(TokenError::InvalidFormat(raw_token.to_string()));
        }
        (vp, Some(fmt))
    } else {
        (inner, None)
    };

    if let Some((root, key)) = val_path.split_once('.') {
        if root.is_empty() || key.is_empty() || root.trim().is_empty() || key.trim().is_empty() {
            return Err(TokenError::EmptySegment(raw_token.to_string()));
        }
        match root {
            "vars" => Ok(Token {
                source: Source::Vars(key),
                format,
                raw,
            }),
            "sys" => {
                if key == "now" {
                    Ok(Token {
                        source: Source::Sys(SysValue::Now),
                        format,
                        raw,
                    })
                } else {
                    Err(TokenError::UnknownSysValue {
                        token: raw_token.to_string(),
                        value: key.to_string(),
                    })
                }
            }
            _ => Err(TokenError::UnknownSource {
                token: raw_token.to_string(),
                source: root.to_string(),
            }),
        }
    } else {
        if !is_valid_ident(val_path) {
            return Err(TokenError::MalformedName(raw_token.to_string()));
        }
        Ok(Token {
            source: Source::Bare(val_path),
            format,
            raw,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScannedToken<'a> {
    pub raw: &'a str,
    pub start: usize,
    pub end: usize,
}

/// Walk an interpolated template string yielding well-formed `{...}` tokens with byte offsets.
/// Honors `{{` and `}}` literal escapes and skips malformed brace sequences.
pub fn scan_tokens(s: &str) -> Vec<ScannedToken<'_>> {
    let mut tokens = Vec::new();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if i + 1 < len && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            i += 2;
            continue;
        }
        if i + 1 < len && bytes[i] == b'}' && bytes[i + 1] == b'}' {
            i += 2;
            continue;
        }
        if bytes[i] == b'{' {
            let mut j = i + 1;
            let mut closed = false;
            while j < len {
                if bytes[j] == b'}' {
                    closed = true;
                    break;
                }
                if bytes[j] == b'{' {
                    break;
                }
                j += 1;
            }
            if closed {
                tokens.push(ScannedToken {
                    raw: &s[i..=j],
                    start: i,
                    end: j + 1,
                });
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    tokens
}

/// Validate the brace syntax of a parameter default string at template load time.
/// Scans for well-formed tokens and verifies that literal chunks between them obey the
/// brace-balance rules (unterminated '{' or unmatched '}'), honouring '{{' and '}}'.
pub fn validate_default_syntax(s: &str) -> Result<(), String> {
    let tokens = scan_tokens(s);
    let mut pos = 0;
    for token in tokens {
        if token.start > pos {
            check_literal_chunk_braces(&s[pos..token.start])?;
        }
        pos = token.end;
    }
    if pos < s.len() {
        check_literal_chunk_braces(&s[pos..])?;
    }
    Ok(())
}

fn check_literal_chunk_braces(chunk: &str) -> Result<(), String> {
    let mut chars = chunk.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                } else {
                    return Err("unterminated '{'".to_string());
                }
            }
            '}' => {
                if chars.peek() == Some(&'}') {
                    chars.next();
                } else {
                    return Err("unmatched '}'".to_string());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_tokens() {
        // Bare
        let t = parse("{title}").unwrap();
        assert_eq!(t.source, Source::Bare("title"));
        assert_eq!(t.format, None);

        let t = parse("{title:short_date}").unwrap();
        assert_eq!(t.source, Source::Bare("title"));
        assert_eq!(t.format, Some("short_date"));

        // Vars
        let t = parse("{vars.qr_base_url}").unwrap();
        assert_eq!(t.source, Source::Vars("qr_base_url"));
        assert_eq!(t.format, None);

        let t = parse("{vars.site.eu.url}").unwrap();
        assert_eq!(t.source, Source::Vars("site.eu.url"));
        assert_eq!(t.format, None);

        let t = parse("{vars.qr_base_url:long_date}").unwrap();
        assert_eq!(t.source, Source::Vars("qr_base_url"));
        assert_eq!(t.format, Some("long_date"));

        // Sys
        let t = parse("{sys.now}").unwrap();
        assert_eq!(t.source, Source::Sys(SysValue::Now));
        assert_eq!(t.format, None);

        let t = parse("{sys.now:iso_date}").unwrap();
        assert_eq!(t.source, Source::Sys(SysValue::Now));
        assert_eq!(t.format, Some("iso_date"));
    }

    #[test]
    fn parse_refusals() {
        // {}
        assert!(matches!(parse("{}"), Err(TokenError::EmptySegment(_))));
        assert!(matches!(parse("{ }"), Err(TokenError::EmptySegment(_))));

        // { id }
        assert!(matches!(parse("{ id }"), Err(TokenError::MalformedName(_))));

        // {my field}
        assert!(matches!(
            parse("{my field}"),
            Err(TokenError::MalformedName(_))
        ));

        // {a.b}
        let err = parse("{a.b}").unwrap_err();
        assert_eq!(
            err,
            TokenError::UnknownSource {
                token: "{a.b}".to_string(),
                source: "a".to_string(),
            }
        );

        // {vars.}
        assert!(matches!(parse("{vars.}"), Err(TokenError::EmptySegment(_))));

        // {vars. }
        assert!(matches!(
            parse("{vars. }"),
            Err(TokenError::EmptySegment(_))
        ));

        // {sys.}
        assert!(matches!(parse("{sys.}"), Err(TokenError::EmptySegment(_))));

        // {sys. }
        assert!(matches!(parse("{sys. }"), Err(TokenError::EmptySegment(_))));

        // {.x}
        assert!(matches!(parse("{.x}"), Err(TokenError::EmptySegment(_))));

        // {:fmt}
        assert!(matches!(parse("{:fmt}"), Err(TokenError::EmptySegment(_))));

        // {x:}
        assert!(matches!(parse("{x:}"), Err(TokenError::InvalidFormat(_))));

        // {x:a:b}
        assert!(matches!(
            parse("{x:a:b}"),
            Err(TokenError::InvalidFormat(_))
        ));

        // {VARS.x}
        let err = parse("{VARS.x}").unwrap_err();
        assert_eq!(
            err,
            TokenError::UnknownSource {
                token: "{VARS.x}".to_string(),
                source: "VARS".to_string(),
            }
        );

        // {Sys.now}
        let err = parse("{Sys.now}").unwrap_err();
        assert_eq!(
            err,
            TokenError::UnknownSource {
                token: "{Sys.now}".to_string(),
                source: "Sys".to_string(),
            }
        );

        // {sys.nwo}
        let err = parse("{sys.nwo}").unwrap_err();
        assert_eq!(
            err,
            TokenError::UnknownSysValue {
                token: "{sys.nwo}".to_string(),
                value: "nwo".to_string(),
            }
        );

        // {sys.now.long_date}
        let err = parse("{sys.now.long_date}").unwrap_err();
        assert_eq!(
            err,
            TokenError::UnknownSysValue {
                token: "{sys.now.long_date}".to_string(),
                value: "now.long_date".to_string(),
            }
        );
    }

    #[test]
    fn scan_tokens_handles_escapes_and_malformed() {
        let text = "Hello {title}, QR is {vars.qr_base_url}/{id}. {{literal}} {sys.now:iso_date} {bad{token} {unterminated";
        let scanned = scan_tokens(text);
        let tokens: Vec<&str> = scanned.iter().map(|t| t.raw).collect();
        assert_eq!(
            tokens,
            vec![
                "{title}",
                "{vars.qr_base_url}",
                "{id}",
                "{sys.now:iso_date}",
                "{token}"
            ]
        );
    }
}
