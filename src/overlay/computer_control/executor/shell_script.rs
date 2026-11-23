//! Free-form PowerShell script text with a UTF-8 output prelude.
//!
//! Windows PowerShell 5.1 encodes redirected output with the OEM code page,
//! which cannot represent most non-ASCII text. The prelude selects UTF-8
//! (no BOM) for the console and for native pipes before the model's command
//! runs. It stays on its own line so the command's own lines keep their text,
//! and it follows any leading `using` statements and script `param` block
//! (with its attributes), which PowerShell requires before every other
//! statement.

const UTF8_PRELUDE: &str = "$sgtUtf8 = [System.Text.UTF8Encoding]::new($false); \
[Console]::OutputEncoding = $sgtUtf8; $OutputEncoding = $sgtUtf8; \
Remove-Variable sgtUtf8\n";

pub(super) fn powershell_script(command: &str) -> String {
    let using_len = leading_using_len(command);
    let head_len = using_len + leading_param_block_len(&command[using_len..]);
    let (head, body) = command.split_at(head_len);
    let separator = if head.is_empty() || head.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    format!("{head}{separator}{UTF8_PRELUDE}{body}")
}

/// Byte length of the leading lines through the last `using` statement.
/// Blank lines and comments may precede or separate those statements.
fn leading_using_len(command: &str) -> usize {
    let mut offset = 0;
    let mut end = 0;
    let mut in_block_comment = false;
    for line in command.split_inclusive('\n') {
        offset += line.len();
        let trimmed = line.trim();
        if in_block_comment {
            in_block_comment = !trimmed.contains("#>");
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("<#") {
            in_block_comment = !rest.contains("#>");
            continue;
        }
        if is_using_statement(trimmed) {
            end = offset;
        } else if !trimmed.is_empty() && !trimmed.starts_with('#') {
            break;
        }
    }
    end
}

fn is_using_statement(line: &str) -> bool {
    let mut words = line.split_whitespace();
    words
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("using"))
        && words.next().is_some_and(|kind| {
            ["namespace", "module", "assembly"]
                .iter()
                .any(|expected| kind.eq_ignore_ascii_case(expected))
        })
}

/// Byte length through the closing parenthesis of a leading script `param`
/// block, including any attributes such as `[CmdletBinding()]` before it.
/// Returns 0 when the text does not start with a param block, so a leading
/// type cast like `[int]$x = 1` stays in the body.
fn leading_param_block_len(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut index = skip_trivia(bytes, 0);
    while bytes.get(index) == Some(&b'[') {
        let Some(end) = balanced_group_end(bytes, index, b'[', b']') else {
            return 0;
        };
        index = skip_trivia(bytes, end);
    }
    let keyword_end = index + "param".len();
    let is_param = bytes
        .get(index..keyword_end)
        .is_some_and(|word| word.eq_ignore_ascii_case(b"param"))
        && bytes
            .get(keyword_end)
            .is_some_and(|next| !next.is_ascii_alphanumeric() && *next != b'_' && *next != b'-');
    if !is_param {
        return 0;
    }
    let open = skip_trivia(bytes, keyword_end);
    if bytes.get(open) != Some(&b'(') {
        return 0;
    }
    balanced_group_end(bytes, open, b'(', b')').unwrap_or(0)
}

/// Skip whitespace, line comments and block comments.
fn skip_trivia(bytes: &[u8], mut index: usize) -> usize {
    loop {
        match bytes.get(index) {
            Some(byte) if byte.is_ascii_whitespace() => index += 1,
            Some(b'#') => {
                index = bytes[index..]
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(bytes.len(), |offset| index + offset + 1);
            }
            Some(b'<') if bytes.get(index + 1) == Some(&b'#') => {
                index = block_comment_end(bytes, index);
            }
            _ => return index,
        }
    }
}

fn block_comment_end(bytes: &[u8], start: usize) -> usize {
    bytes[start + 2..]
        .windows(2)
        .position(|pair| pair == b"#>")
        .map_or(bytes.len(), |offset| start + 2 + offset + 2)
}

/// End offset (exclusive) of the bracketed group opened at `start`, skipping
/// quoted strings and comments. `None` when the group never closes.
fn balanced_group_end(bytes: &[u8], start: usize, open: u8, close: u8) -> Option<usize> {
    let mut depth = 0_usize;
    let mut index = start;
    while let Some(&byte) = bytes.get(index) {
        match byte {
            b'\'' => index = single_quoted_end(bytes, index)?,
            b'"' => index = double_quoted_end(bytes, index)?,
            b'#' => {
                index = skip_trivia(bytes, index);
                continue;
            }
            b'<' if bytes.get(index + 1) == Some(&b'#') => {
                index = block_comment_end(bytes, index);
                continue;
            }
            _ if byte == open => depth += 1,
            _ if byte == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// Index of the closing quote of a single-quoted string ('' escapes a quote).
fn single_quoted_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start + 1;
    loop {
        match bytes.get(index)? {
            b'\'' if bytes.get(index + 1) == Some(&b'\'') => index += 2,
            b'\'' => return Some(index),
            _ => index += 1,
        }
    }
}

/// Index of the closing quote of a double-quoted string (backtick escapes and
/// a doubled quote both escape a quote).
fn double_quoted_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start + 1;
    loop {
        match bytes.get(index)? {
            b'`' => index += 2,
            b'"' if bytes.get(index + 1) == Some(&b'"') => index += 2,
            b'"' => return Some(index),
            _ => index += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{UTF8_PRELUDE, powershell_script};

    #[test]
    fn prelude_leads_ordinary_commands() {
        assert_eq!(
            powershell_script("Write-Output x"),
            format!("{UTF8_PRELUDE}Write-Output x")
        );
        assert_eq!(
            powershell_script("$using = 1\nusing"),
            format!("{UTF8_PRELUDE}$using = 1\nusing")
        );
    }

    #[test]
    fn prelude_follows_every_leading_using_statement() {
        let head = "# setup\r\n<# block\r\n using namespace Hidden #>\r\nUSING namespace System.Text\r\n\r\nusing Assembly System.Web\r\n";
        let body = "Write-Output x\r\nusing namespace Late";
        assert_eq!(
            powershell_script(&format!("{head}{body}")),
            format!("{head}{UTF8_PRELUDE}{body}")
        );
        assert_eq!(
            powershell_script("using module Example"),
            format!("using module Example\n{UTF8_PRELUDE}")
        );
    }

    #[test]
    fn prelude_follows_a_leading_param_block_and_its_attributes() {
        let head = "using namespace System.Text\n# options\n[CmdletBinding()]\n\
            [Diagnostics.CodeAnalysis.SuppressMessage('x', '')]\nparam(\n  \
            [string]$Name = 'a)b''c', # trailing ) comment\n  \
            [string]$Other = \"q`\")\" <# ) #>\n)";
        let body = " Write-Output $Name";
        assert_eq!(
            powershell_script(&format!("{head}{body}")),
            format!("{head}\n{UTF8_PRELUDE}{body}")
        );
        assert_eq!(
            powershell_script("PARAM ($x)\n$x"),
            format!("PARAM ($x)\n{UTF8_PRELUDE}\n$x")
        );
    }

    #[test]
    fn non_param_leading_text_keeps_the_prelude_first() {
        for command in [
            "[int]$x = 1; $x",
            "params = 1",
            "param-Thing",
            "[CmdletBinding()] Write-Output x",
            "param($unclosed",
        ] {
            assert_eq!(
                powershell_script(command),
                format!("{UTF8_PRELUDE}{command}"),
                "{command}"
            );
        }
    }
}
