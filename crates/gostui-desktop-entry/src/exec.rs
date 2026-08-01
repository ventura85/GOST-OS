//! `Exec=` parsing and field-code expansion.
//!
//! Two separate jobs, done in this order:
//!
//! 1. **Tokenise** the `Exec` string using the quoting rules from the Desktop
//!    Entry Specification. Getting this wrong means paths with spaces silently
//!    launch the wrong thing.
//! 2. **Expand field codes** (`%f`, `%U`, `%i`, …) against the files the user
//!    actually dropped on the shortcut.
//!
//! Expansion happens exactly once, and the expanded values are never re-scanned
//! for field codes. A filename containing `%F` must stay a filename.

/// A field code we recognise but that carries no meaning any more.
/// The specification says these must be ignored, not passed through.
const DEPRECATED: &[char] = &['d', 'D', 'n', 'N', 'v', 'm'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecError {
    /// A quoted argument was never closed.
    UnterminatedQuote,
    /// A field code that is not in the specification.
    UnknownFieldCode(char),
    /// The `Exec` value had no command at all.
    Empty,
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnterminatedQuote => write!(f, "unterminated quote in Exec"),
            Self::UnknownFieldCode(c) => write!(f, "unknown field code %{c} in Exec"),
            Self::Empty => write!(f, "empty Exec value"),
        }
    }
}

impl std::error::Error for ExecError {}

/// Split an `Exec` value into arguments, honouring the specification's quoting.
///
/// Inside double quotes, a backslash escapes `"`, `` ` ``, `$` and `\`.
pub fn tokenize(exec: &str) -> Result<Vec<String>, ExecError> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut has_current = false;
    let mut chars = exec.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => {
                if has_current {
                    args.push(std::mem::take(&mut current));
                    has_current = false;
                }
            }
            '"' => {
                has_current = true;
                let mut closed = false;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => {
                            closed = true;
                            break;
                        }
                        '\\' => match chars.next() {
                            Some(esc @ ('"' | '`' | '$' | '\\')) => current.push(esc),
                            Some(other) => {
                                current.push('\\');
                                current.push(other);
                            }
                            None => return Err(ExecError::UnterminatedQuote),
                        },
                        other => current.push(other),
                    }
                }
                if !closed {
                    return Err(ExecError::UnterminatedQuote);
                }
            }
            other => {
                has_current = true;
                current.push(other);
            }
        }
    }
    if has_current {
        args.push(current);
    }
    if args.is_empty() {
        return Err(ExecError::Empty);
    }
    Ok(args)
}

/// What the field codes should be filled in with.
#[derive(Debug, Clone, Default)]
pub struct ExecContext<'a> {
    /// Local paths the user is opening. Used for `%f` and `%F`.
    pub files: &'a [String],
    /// URIs the user is opening. Used for `%u` and `%U`; falls back to `files`
    /// when empty, since a local path is a valid URI target.
    pub uris: &'a [String],
    /// Value of `Icon=`, for `%i`.
    pub icon: Option<&'a str>,
    /// Translated `Name=`, for `%c`.
    pub name: Option<&'a str>,
    /// Path of the `.desktop` file itself, for `%k`.
    pub desktop_file: Option<&'a str>,
}

/// Expand field codes in already-tokenised arguments.
///
/// List codes (`%F`, `%U`) expand one argument into many; single codes (`%f`,
/// `%u`) expand into one, or drop the argument when there is nothing to put there.
pub fn expand(args: &[String], ctx: &ExecContext<'_>) -> Result<Vec<String>, ExecError> {
    let uris: &[String] = if ctx.uris.is_empty() {
        ctx.files
    } else {
        ctx.uris
    };
    let mut out = Vec::with_capacity(args.len());

    for arg in args {
        if !arg.contains('%') {
            out.push(arg.clone());
            continue;
        }

        // A whole argument that is exactly one list code expands into many args.
        match arg.as_str() {
            "%F" => {
                out.extend(ctx.files.iter().cloned());
                continue;
            }
            "%U" => {
                out.extend(uris.iter().cloned());
                continue;
            }
            "%i" => {
                if let Some(icon) = ctx.icon {
                    out.push("--icon".to_string());
                    out.push(icon.to_string());
                }
                continue;
            }
            _ => {}
        }

        let mut expanded = String::with_capacity(arg.len());
        let mut dropped = false;
        let mut chars = arg.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '%' {
                expanded.push(c);
                continue;
            }
            match chars.next() {
                Some('%') => expanded.push('%'),
                Some('f') => match ctx.files.first() {
                    Some(f) => expanded.push_str(f),
                    None => dropped = true,
                },
                Some('u') => match uris.first() {
                    Some(u) => expanded.push_str(u),
                    None => dropped = true,
                },
                Some('c') => expanded.push_str(ctx.name.unwrap_or_default()),
                Some('k') => expanded.push_str(ctx.desktop_file.unwrap_or_default()),
                // Embedded list codes are not meaningful inside a larger argument;
                // the specification only allows them standalone. Drop them.
                Some('F' | 'U' | 'i') => dropped = true,
                Some(d) if DEPRECATED.contains(&d) => {}
                Some(other) => return Err(ExecError::UnknownFieldCode(other)),
                None => expanded.push('%'),
            }
        }
        // An argument that expanded to nothing is removed, not passed on as an
        // empty string. "%f" with no file gives nothing; "--file=%f" with no file
        // gives nothing either, rather than a truncated flag; and an argument made
        // only of deprecated codes disappears entirely.
        if dropped || expanded.is_empty() {
            continue;
        }
        out.push(expanded);
    }

    if out.is_empty() {
        return Err(ExecError::Empty);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with_files(files: &[String]) -> ExecContext<'_> {
        ExecContext {
            files,
            ..Default::default()
        }
    }

    #[test]
    fn quoted_paths_with_spaces_stay_one_argument() {
        let args = tokenize(r#"/usr/bin/app "My Documents/report.pdf""#).unwrap();
        assert_eq!(args, ["/usr/bin/app", "My Documents/report.pdf"]);
    }

    #[test]
    fn backslash_escapes_inside_quotes() {
        let args = tokenize(r#"app "a\"b" "c\\d""#).unwrap();
        assert_eq!(args, ["app", r#"a"b"#, r"c\d"]);
    }

    #[test]
    fn an_unterminated_quote_is_an_error_not_a_panic() {
        assert_eq!(tokenize(r#"app "oops"#), Err(ExecError::UnterminatedQuote));
    }

    #[test]
    fn empty_exec_is_an_error() {
        assert_eq!(tokenize("   "), Err(ExecError::Empty));
    }

    #[test]
    fn list_code_expands_to_every_file() {
        let files = vec!["a.txt".to_string(), "b.txt".to_string()];
        let args = tokenize("editor %F").unwrap();
        assert_eq!(
            expand(&args, &ctx_with_files(&files)).unwrap(),
            ["editor", "a.txt", "b.txt"]
        );
    }

    #[test]
    fn single_code_takes_only_the_first_file() {
        let files = vec!["a.txt".to_string(), "b.txt".to_string()];
        let args = tokenize("viewer %f").unwrap();
        assert_eq!(
            expand(&args, &ctx_with_files(&files)).unwrap(),
            ["viewer", "a.txt"]
        );
    }

    #[test]
    fn launching_with_no_files_drops_the_placeholder() {
        let args = tokenize("editor %F").unwrap();
        assert_eq!(expand(&args, &ExecContext::default()).unwrap(), ["editor"]);
        let args = tokenize("editor %f").unwrap();
        assert_eq!(expand(&args, &ExecContext::default()).unwrap(), ["editor"]);
    }

    #[test]
    fn a_partial_flag_is_dropped_whole_rather_than_left_truncated() {
        let args = tokenize("app --file=%f").unwrap();
        assert_eq!(expand(&args, &ExecContext::default()).unwrap(), ["app"]);
    }

    #[test]
    fn urls_fall_back_to_file_paths() {
        let files = vec!["/home/u/a.pdf".to_string()];
        let args = tokenize("browser %U").unwrap();
        assert_eq!(
            expand(&args, &ctx_with_files(&files)).unwrap(),
            ["browser", "/home/u/a.pdf"]
        );
    }

    #[test]
    fn icon_code_expands_to_a_flag_pair_or_nothing() {
        let args = tokenize("app %i").unwrap();
        let ctx = ExecContext {
            icon: Some("firefox"),
            ..Default::default()
        };
        assert_eq!(expand(&args, &ctx).unwrap(), ["app", "--icon", "firefox"]);
        assert_eq!(expand(&args, &ExecContext::default()).unwrap(), ["app"]);
    }

    #[test]
    fn percent_percent_is_a_literal_percent() {
        let args = tokenize("app 100%%").unwrap();
        assert_eq!(
            expand(&args, &ExecContext::default()).unwrap(),
            ["app", "100%"]
        );
    }

    #[test]
    fn deprecated_codes_are_ignored() {
        let args = tokenize("app %d %n").unwrap();
        assert_eq!(expand(&args, &ExecContext::default()).unwrap(), ["app"]);
    }

    #[test]
    fn an_unknown_field_code_is_reported() {
        let args = tokenize("app %z").unwrap();
        assert_eq!(
            expand(&args, &ExecContext::default()),
            Err(ExecError::UnknownFieldCode('z'))
        );
    }

    #[test]
    fn a_filename_containing_a_field_code_is_not_expanded_again() {
        // The whole reason expansion is a single pass.
        let files = vec!["weird %F name.txt".to_string()];
        let args = tokenize("editor %f").unwrap();
        assert_eq!(
            expand(&args, &ctx_with_files(&files)).unwrap(),
            ["editor", "weird %F name.txt"]
        );
    }
}
