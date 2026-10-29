use crate::CompilerError;
use swc_common::Spanned;
use swc_ecma_parser::input::Tokens;
use swc_ecma_parser::{Parser, StringInput, Syntax, lexer::Lexer};
pub(crate) fn parse_module(
    source: &crate::source::Source,
) -> Result<swc_ecma_ast::Module, CompilerError> {
    let fm = &source.file;
    check_complexity(source)?;

    let lexer = Lexer::new(
        Syntax::Typescript(Default::default()),
        Default::default(),
        StringInput::from(&**fm),
        None,
    );

    let mut parser = Parser::new_from(lexer);
    let module = parser.parse_typescript_module().map_err(|error| {
        CompilerError::Parse(Box::new(source.diagnostic(
            varta_common::DiagnosticCode::SyntaxError,
            error.kind().msg().into_owned(),
            error.span(),
        )))
    })?;
    if let Some(error) = parser.take_errors().into_iter().next() {
        return Err(CompilerError::Parse(Box::new(source.diagnostic(
            varta_common::DiagnosticCode::SyntaxError,
            error.kind().msg().into_owned(),
            error.span(),
        ))));
    }
    Ok(module)
}

/// Nested delimiters bound recursive descent. A run of tokens without a sibling
/// separator bounds operator, member and unary chains, which build nested AST
/// nodes. Flat sibling lists do not deepen the AST and reset the run: commas
/// inside `[...]`/`{...}` (call arguments keep the chain budget) and, at the top
/// level of a type alias, union/intersection members, which the parser collects
/// in a loop. Overall size is bounded by the per-file and per-graph byte limits.
const MAX_NESTING: usize = 64;
const MAX_CHAIN_TOKENS: usize = 256;

/// Tracks `type Name = ...` so `|`/`&` reset the run only in its own type position.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Alias {
    Outside,
    Keyword,
    Name,
    Type(usize),
}

/// Opens a template substitution; template head and middle tokens end with it.
const SUBSTITUTION: &str = "${";

fn check_complexity(source: &crate::source::Source) -> Result<(), CompilerError> {
    let fm = &source.file;
    let mut tokens = Lexer::new(
        Syntax::Typescript(Default::default()),
        Default::default(),
        StringInput::from(&**fm),
        None,
    );
    let slice = |span: swc_common::Span| {
        let start = (span.lo.0 - fm.start_pos.0) as usize;
        let end = (span.hi.0 - fm.start_pos.0) as usize;
        fm.src.get(start..end).unwrap_or_default()
    };
    let mut nesting = 0usize;
    let mut run = 0usize;
    let mut delimiters = Vec::new();
    let mut has_angle = false;
    let mut alias = Alias::Outside;
    let mut previous = "";
    while let Some(mut token) = tokens.next() {
        let mut text = slice(token.span);
        // A backtick starts a template token that ends at the closing backtick
        // or at its first substitution.
        let mut template = text.starts_with('`');
        if text == "}"
            && delimiters
                .last()
                .is_some_and(|(open, _)| *open == SUBSTITUTION)
        {
            // The `}` closing a substitution resumes the template text, which
            // only a template rescan lexes (as the parser does); otherwise the
            // rest of the template, and later comments, would lex as code.
            nesting = nesting.saturating_sub(1);
            delimiters.pop();
            token = tokens.rescan_template_token(token.span.lo, false);
            text = slice(token.span);
            template = true;
        }
        if let Alias::Type(depth) = alias
            && (nesting < depth
                || nesting == depth
                    && (previous == ";"
                        || token.had_line_break
                            && !matches!(text, "|" | "&")
                            && !matches!(previous, "=" | "|" | "&")))
        {
            // The alias ended: an explicit `;`, an enclosing close, or ASI at a
            // line break that does not continue the union/intersection.
            alias = Alias::Outside;
        }
        alias = match (alias, text) {
            (Alias::Type(depth), _) => Alias::Type(depth),
            (_, "type")
                if matches!(previous, "" | ";" | "{" | "}" | "export" | "declare")
                    || token.had_line_break =>
            {
                Alias::Keyword
            }
            (Alias::Keyword, _) if is_identifier(text) => Alias::Name,
            (Alias::Name, "=") => Alias::Type(nesting),
            _ => Alias::Outside,
        };
        match text {
            _ if template => {
                if text.ends_with(SUBSTITUTION) {
                    nesting += 1;
                    delimiters.push((SUBSTITUTION, has_angle));
                }
            }
            "(" | "[" | "{" => {
                nesting += 1;
                delimiters.push((text, has_angle));
                if text == "[" {
                    has_angle = false;
                }
            }
            ")" | "]" | "}" => {
                nesting = nesting.saturating_sub(1);
                if let Some(("[", outer_angle)) = delimiters.pop() {
                    has_angle |= outer_angle;
                }
            }
            _ => {}
        }
        // Do not treat separators inside generic angle syntax as boundaries.
        has_angle |= text.starts_with('<') || text.starts_with('>');
        let boundary = matches!(text, ";" | "{" | "}");
        let list_comma = text == ","
            && delimiters
                .last()
                .is_some_and(|(open, _)| matches!(*open, "[" | "{"));
        let type_member = matches!(text, "|" | "&") && alias == Alias::Type(nesting);
        run = if boundary || (list_comma || type_member) && !has_angle {
            0
        } else {
            run + 1
        };
        if boundary {
            has_angle = false;
        }
        if nesting > MAX_NESTING || run > MAX_CHAIN_TOKENS {
            return Err(source.locate(
                CompilerError::Invalid("source complexity exceeds compiler limits".into()),
                token.span,
            ));
        }
        previous = text;
    }
    Ok(())
}

fn is_identifier(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}
