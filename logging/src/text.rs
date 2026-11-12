//! Best-effort credential redaction shared by log lines and troubleshooting
//! text. It handles command switches, URL query fields and JSON-style fields;
//! it does not claim to identify an arbitrary secret without a named field.

fn sensitive(key: &str) -> bool {
    let key = key.trim_start_matches('-').to_ascii_lowercase().replace('-', "_");
    key.contains("password")
        || key.contains("passwd")
        || key.contains("token")
        || key.contains("secret")
        || matches!(key.as_str(), "auth" | "authorization" | "authentication" | "login" | "credential" | "credentials" | "ticket" | "access_key" | "api_key")
        || key == "paradox_account_userid"
        || key.starts_with("auth_")
        || key.ends_with("_auth")
        || key.ends_with("_ticket")
}

/// Redacts known credential fields without printing or persisting their value.
/// Noncredential switches such as `-userdir` and `-harmony_core` remain useful.
pub fn redact(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let boundary = at == 0 || bytes[at - 1].is_ascii_whitespace() || matches!(bytes[at - 1], b'?' | b'&' | b'"' | b'\'' | b'{');
        if !boundary || !(bytes[at].is_ascii_alphabetic() || bytes[at] == b'-' || bytes[at] == b'_') {
            at += 1;
            continue;
        }
        let start = at;
        while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'-' | b'_')) {
            at += 1;
        }
        if !sensitive(&text[start..at]) {
            continue;
        }
        let mut next = at;
        if bytes.get(next).is_some_and(|b| matches!(b, b'"' | b'\'')) {
            next += 1;
        }
        let separated = bytes.get(next).is_some_and(|b| b.is_ascii_whitespace() || matches!(b, b'=' | b':'));
        if !separated {
            continue;
        }
        while bytes.get(next).is_some_and(|b| b.is_ascii_whitespace() || matches!(b, b'=' | b':')) {
            next += 1;
        }
        if next >= bytes.len() {
            continue;
        }
        let quote = bytes.get(next).copied().filter(|b| matches!(b, b'"' | b'\''));
        if quote.is_some() {
            next += 1;
        }
        let value = next;
        if let Some(quote) = quote {
            while next < bytes.len() && bytes[next] != quote {
                if bytes[next] == b'\\' && next + 1 < bytes.len() {
                    next += 2;
                } else {
                    next += 1;
                }
            }
        } else {
            while next < bytes.len() && !bytes[next].is_ascii_whitespace() && !matches!(bytes[next], b'&' | b',' | b'}' | b'"' | b'\'') {
                next += 1;
            }
            if text[start..at].eq_ignore_ascii_case("authorization") && text[value..next].eq_ignore_ascii_case("bearer") {
                while bytes.get(next).is_some_and(u8::is_ascii_whitespace) {
                    next += 1;
                }
                while next < bytes.len() && !bytes[next].is_ascii_whitespace() && !matches!(bytes[next], b',' | b'}' | b'"') {
                    next += 1;
                }
            }
        }
        if next > value {
            ranges.push(value..next);
        }
        at = next.max(at);
    }
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for range in ranges {
        if range.start < copied {
            continue;
        }
        out.push_str(&text[copied..range.start]);
        out.push_str("[redacted]");
        copied = range.end;
    }
    out.push_str(&text[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_json_and_url_credentials_are_removed_without_losing_nonsecret_context() {
        let line = r#"eu4.exe -userdir="C:\my games" -auth_token="one two" --password=three -harmony_core=D:\core.dll -ticket four"#;
        let got = redact(line);
        assert!(!got.contains("one two"));
        assert!(!got.contains("three"));
        assert!(!got.contains(" four"));
        assert!(got.contains("C:\\my games"));
        assert!(got.contains("D:\\core.dll"));
        assert_eq!(redact(r#"{"access_token":"abc","ordinary":"ok"}"#), r#"{"access_token":"[redacted]","ordinary":"ok"}"#);
        assert_eq!(redact("https://x/?token=abc&keep=yes"), "https://x/?token=[redacted]&keep=yes");
        assert_eq!(redact("Authorization: Bearer abc\nnext"), "Authorization: [redacted]\nnext");
        assert_eq!(redact(r#"{"password":"one\"two","keep":1}"#), r#"{"password":"[redacted]","keep":1}"#);
        assert_eq!(
            redact("eu4.exe --pdx-launcher-session-token example-token --paradox-account-userid example-account"),
            "eu4.exe --pdx-launcher-session-token [redacted] --paradox-account-userid [redacted]"
        );
        assert_eq!(redact("a normal error in rendering"), "a normal error in rendering");
    }
}
