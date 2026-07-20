//! Shell completion foundation — platform-agnostic types and helpers
//! extracted from shell-invocation code so they can be unit-tested without
//! a real bash/GitBash/PS process (P12-SC-foundation).

/// Shell-independent token parsed from a command-line input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub value: String,
    pub start: usize,
    pub end: usize,
    pub quote: QuoteStyle,
}

impl Token {
    pub fn new(value: String, start: usize, end: usize, quote: QuoteStyle) -> Self {
        Self {
            value,
            start,
            end,
            quote,
        }
    }

    /// Re-compute `end` after `value` has been mutated.
    pub fn refresh_end(&mut self) {
        self.end = self.start + self.value.len();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteStyle {
    None,
    Double,
    Single,
}

/// The replacement range and text for a selected completion candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

// ---------------------------------------------------------------------------
// ShellCompletionAdapter — platform adapter trait
// ---------------------------------------------------------------------------

/// Platform-specific completion behavior.
///
/// Every method is a pure computation over strings; the adapter is
/// instantiated without spawning a shell or querying the OS.
pub trait ShellCompletionAdapter: Send + Sync {
    /// Split `input` into tokens using the shell's quoting rules.
    fn tokenize(&self, input: &str) -> Vec<Token>;

    /// Rank a candidate against the typed prefix (higher = better match).
    fn score(&self, candidate: &str, prefix: &str) -> usize;

    /// Produce the replacement text given the original input, the token
    /// being replaced, and the chosen candidate value.
    fn apply_replacement(&self, input: &str, token: &Token, candidate: &str) -> Replacement;

    /// Escape a value so the shell interprets it as a single literal argument.
    fn escape(&self, value: &str) -> String;
}

// ---------------------------------------------------------------------------
// tokenize — basic whitespace-and-quote tokenizer
// ---------------------------------------------------------------------------

/// Minimal shell-agnostic tokenizer.
///
/// Splits `input` on whitespace boundaries while tracking single/double
/// quotes. This is the common subset shared by POSIX shells, PowerShell,
/// and cmd.exe; each [`ShellCompletionAdapter`] may refine it.
pub fn tokenize(input: &str) -> Vec<Token> {
    tokenize_impl(input, b'\\')
}

/// Shared tokenizer core: `escape_byte` is `b'\\'` for POSIX, `` b'`' `` for PowerShell.
fn tokenize_impl(input: &str, escape_byte: u8) -> Vec<Token> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }

        let start = i;
        let mut value = String::new();
        let mut quote = QuoteStyle::None;

        while i < len {
            let b = bytes[i];
            if b == b'"' {
                match quote {
                    QuoteStyle::None => {
                        quote = QuoteStyle::Double;
                        i += 1;
                        continue;
                    }
                    QuoteStyle::Double => {
                        quote = QuoteStyle::None;
                        i += 1;
                        continue;
                    }
                    QuoteStyle::Single => {}
                }
            } else if b == b'\'' {
                match quote {
                    QuoteStyle::None => {
                        quote = QuoteStyle::Single;
                        i += 1;
                        continue;
                    }
                    QuoteStyle::Single => {
                        quote = QuoteStyle::None;
                        i += 1;
                        continue;
                    }
                    QuoteStyle::Double => {}
                }
            } else if b == escape_byte && quote == QuoteStyle::None && i + 1 < len {
                i += 1;
                let next = bytes[i];
                value.push(next as char);
                i += 1;
                continue;
            }

            if b.is_ascii_whitespace() && quote == QuoteStyle::None {
                break;
            }

            // Multi-byte UTF-8: advance by the char's byte length.
            let ch = input[i..].chars().next().unwrap_or('\0');
            let char_len = ch.len_utf8();
            value.push(ch);
            i += char_len;
        }

        tokens.push(Token::new(value, start, i, quote));
    }

    tokens
}

// ---------------------------------------------------------------------------
// score — prefix-based candidate ranking
// ---------------------------------------------------------------------------

/// Rank a candidate by how well it matches `prefix`.
///
/// Returns a score where:
/// - exact match (case-insensitive) = 100
/// - prefix match = 50
/// - substring match = 25
/// - no match = 0
pub fn score(candidate: &str, prefix: &str) -> usize {
    if prefix.is_empty() {
        return 10;
    }

    let lower_cand = candidate.to_ascii_lowercase();
    let lower_pre = prefix.to_ascii_lowercase();

    if lower_cand == lower_pre {
        100
    } else if lower_cand.starts_with(&lower_pre) {
        50
    } else if lower_cand.contains(&lower_pre) {
        25
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// apply_replacement — compute replacement range and text
// ---------------------------------------------------------------------------

/// Produce a [`Replacement`] by replacing the token's range in `input`
/// with the shell-escaped `candidate`.
pub fn apply_replacement(input: &str, token: &Token, candidate: &str) -> Replacement {
    Replacement {
        start: token.start,
        end: token.end,
        text: candidate.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// escape — shell-agnostic escaping
// ---------------------------------------------------------------------------

/// Shell-agnostic default escaping: wrap in single quotes, replacing
/// embedded single quotes with `'\''` (the POSIX idiom).
///
/// Platform adapters may override for their quoting rules.
pub fn escape(value: &str) -> String {
    if value.is_empty() {
        return String::from("''");
    }
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

// ---------------------------------------------------------------------------
// POSIX ShellCompletionAdapter
// ---------------------------------------------------------------------------

pub struct PosixAdapter;

impl ShellCompletionAdapter for PosixAdapter {
    fn tokenize(&self, input: &str) -> Vec<Token> {
        tokenize(input)
    }

    fn score(&self, candidate: &str, prefix: &str) -> usize {
        score(candidate, prefix)
    }

    fn apply_replacement(&self, input: &str, token: &Token, candidate: &str) -> Replacement {
        apply_replacement(input, token, candidate)
    }

    fn escape(&self, value: &str) -> String {
        escape(value)
    }
}

// ---------------------------------------------------------------------------
// PowerShell ShellCompletionAdapter
// ---------------------------------------------------------------------------

pub struct PowerShellAdapter;

impl ShellCompletionAdapter for PowerShellAdapter {
    fn tokenize(&self, input: &str) -> Vec<Token> {
        // PowerShell uses backtick as escape, not backslash.
        tokenize_impl(input, b'`')
    }

    fn score(&self, candidate: &str, prefix: &str) -> usize {
        score(candidate, prefix)
    }

    fn apply_replacement(&self, input: &str, token: &Token, candidate: &str) -> Replacement {
        apply_replacement(input, token, candidate)
    }

    fn escape(&self, value: &str) -> String {
        // PowerShell escaping: wrap in single quotes, double embedded quotes.
        if value.is_empty() {
            return String::from("''");
        }
        let mut out = String::with_capacity(value.len() + 2);
        out.push('\'');
        for ch in value.chars() {
            if ch == '\'' {
                out.push_str("''");
            } else {
                out.push(ch);
            }
        }
        out.push('\'');
        out
    }
}

// ---------------------------------------------------------------------------
// cmd.exe ShellCompletionAdapter
// ---------------------------------------------------------------------------

pub struct CmdAdapter;

impl ShellCompletionAdapter for CmdAdapter {
    fn tokenize(&self, input: &str) -> Vec<Token> {
        // cmd.exe does not use single or double quotes for argument
        // boundary — only whitespace splits tokens.
        let mut tokens = Vec::new();
        let bytes = input.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            if bytes[i].is_ascii_whitespace() {
                i += 1;
                continue;
            }

            let start = i;
            while i < len && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }

            tokens.push(Token::new(
                input[start..i].to_owned(),
                start,
                i,
                QuoteStyle::None,
            ));
        }

        tokens
    }

    fn score(&self, candidate: &str, prefix: &str) -> usize {
        score(candidate, prefix)
    }

    fn apply_replacement(&self, input: &str, token: &Token, candidate: &str) -> Replacement {
        apply_replacement(input, token, candidate)
    }

    fn escape(&self, value: &str) -> String {
        // cmd.exe escaping: wrap in double quotes, double embedded quotes.
        if value.is_empty() {
            return String::from("\"\"");
        }
        let mut out = String::with_capacity(value.len() + 4);
        out.push('"');
        for ch in value.chars() {
            if ch == '"' {
                out.push_str("\"\"");
            } else {
                out.push(ch);
            }
        }
        out.push('"');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- tokenize ---

    #[test]
    fn tokenize_empty() {
        assert!(tokenize("").is_empty());
    }

    #[test]
    fn tokenize_single_word() {
        let t = tokenize("foo");
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].value, "foo");
        assert_eq!(t[0].start, 0);
        assert_eq!(t[0].end, 3);
    }

    #[test]
    fn tokenize_multiple_words() {
        let t = tokenize("foo bar baz");
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].value, "foo");
        assert_eq!(t[1].value, "bar");
        assert_eq!(t[2].value, "baz");
    }

    #[test]
    fn tokenize_double_quoted() {
        let t = tokenize("echo \"hello world\"");
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].value, "hello world");
    }

    #[test]
    fn tokenize_single_quoted() {
        let t = tokenize("echo 'it\\s fine'");
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].value, "it\\s fine");
    }

    #[test]
    fn tokenize_backslash_escape() {
        let t = tokenize("echo hello\\ world");
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].value, "hello world");
    }

    // --- score ---

    #[test]
    fn score_exact_match() {
        assert_eq!(score("hello", "hello"), 100);
    }

    #[test]
    fn score_exact_case_insensitive() {
        assert_eq!(score("Hello", "hello"), 100);
    }

    #[test]
    fn score_prefix_match() {
        assert_eq!(score("hello_world", "hel"), 50);
    }

    #[test]
    fn score_substring_match() {
        assert_eq!(score("my_hello_world", "hello"), 25);
    }

    #[test]
    fn score_no_match() {
        assert_eq!(score("hello", "xyz"), 0);
    }

    #[test]
    fn score_empty_prefix() {
        assert_eq!(score("anything", ""), 10);
    }

    // --- apply_replacement ---

    #[test]
    fn replace_token_in_middle() {
        let token = Token::new("bar".into(), 4, 7, QuoteStyle::None);
        let r = apply_replacement("foo bar baz", &token, "newbar");
        assert_eq!(r.start, 4);
        assert_eq!(r.end, 7);
        assert_eq!(r.text, "newbar");
    }

    // --- escape ---

    #[test]
    fn escape_plain() {
        assert_eq!(escape("hello"), "'hello'");
    }

    #[test]
    fn escape_with_single_quote() {
        assert_eq!(escape("it's"), "'it'\\''s'");
    }

    #[test]
    fn escape_empty() {
        assert_eq!(escape(""), "''");
    }

    // --- PosixAdapter ---

    #[test]
    fn posix_tokenize() {
        let adapter = PosixAdapter;
        let t = adapter.tokenize("echo hello");
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn posix_score() {
        let adapter = PosixAdapter;
        assert_eq!(adapter.score("hello", "hel"), 50);
    }

    #[test]
    fn posix_replace() {
        let adapter = PosixAdapter;
        let token = Token::new("foo".into(), 0, 3, QuoteStyle::None);
        let r = adapter.apply_replacement("foo bar", &token, "newfoo");
        assert_eq!(r.text, "newfoo");
    }

    #[test]
    fn posix_escape() {
        let adapter = PosixAdapter;
        assert_eq!(adapter.escape("test"), "'test'");
    }

    // --- PowerShellAdapter ---

    #[test]
    fn powershell_backtick_escape() {
        let adapter = PowerShellAdapter;
        let t = adapter.tokenize("echo hello` world");
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].value, "hello world");
    }

    #[test]
    fn powershell_escape_single_quote() {
        let adapter = PowerShellAdapter;
        assert_eq!(adapter.escape("it's"), "'it''s'");
    }

    #[test]
    fn powershell_tokenize_path_with_spaces() {
        let adapter = PowerShellAdapter;
        let t = adapter.tokenize("cd 'C:\\Program Files'");
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].value, "C:\\Program Files");
    }

    #[test]
    fn powershell_tokenize_unicode_path() {
        let adapter = PowerShellAdapter;
        let t = adapter.tokenize("ls 'C:\\Users\\Jürgen\\文件'");
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].value, "C:\\Users\\Jürgen\\文件");
    }

    #[test]
    fn powershell_tokenize_drive_letter() {
        let adapter = PowerShellAdapter;
        let t = adapter.tokenize("cd D:\\Projects");
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].value, "D:\\Projects");
    }

    #[test]
    fn powershell_tokenize_unc_path() {
        let adapter = PowerShellAdapter;
        let t = adapter.tokenize("ls \\\\server\\share\\folder");
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].value, "\\\\server\\share\\folder");
    }

    #[test]
    fn powershell_escape_path_with_spaces() {
        let adapter = PowerShellAdapter;
        let escaped = adapter.escape("C:\\Program Files\\app.exe");
        assert_eq!(escaped, "'C:\\Program Files\\app.exe'");
    }

    // --- CmdAdapter ---

    #[test]
    fn cmd_tokenize() {
        let adapter = CmdAdapter;
        let t = adapter.tokenize("dir C:\\Users");
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].value, "C:\\Users");
    }

    #[test]
    fn cmd_escape_double_quote() {
        let adapter = CmdAdapter;
        assert_eq!(adapter.escape("hello \"world\""), "\"hello \"\"world\"\"\"");
    }

    #[test]
    fn cmd_tokenize_path_with_spaces() {
        let adapter = CmdAdapter;
        // cmd.exe splits on whitespace only; quotes are literal chars.
        let t = adapter.tokenize("dir C:\\Program Files");
        assert_eq!(t.len(), 3);
        assert_eq!(t[1].value, "C:\\Program");
        assert_eq!(t[2].value, "Files");
    }

    #[test]
    fn cmd_tokenize_unicode() {
        let adapter = CmdAdapter;
        let t = adapter.tokenize("dir C:\\Users\\Jürgen");
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].value, "C:\\Users\\Jürgen");
    }

    #[test]
    fn cmd_escape_path_with_spaces() {
        let adapter = CmdAdapter;
        let escaped = adapter.escape("C:\\Program Files\\app.exe");
        assert_eq!(escaped, "\"C:\\Program Files\\app.exe\"");
    }

    #[test]
    fn cmd_escape_empty() {
        let adapter = CmdAdapter;
        assert_eq!(adapter.escape(""), "\"\"");
    }
}
