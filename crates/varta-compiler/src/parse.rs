use crate::CompilerError;
use swc_common::Spanned;
use swc_ecma_parser::{Parser, StringInput, Syntax, lexer::Lexer};
pub(crate) fn parse_module(
    source: &crate::source::Source,
) -> Result<swc_ecma_ast::Module, CompilerError> {
    let fm = &source.file;
    // Bound recursion before constructing an AST. Token spans distinguish punctuation
    // from the same characters inside strings and comments.
    let tokens = Lexer::new(
        Syntax::Typescript(Default::default()),
        Default::default(),
        StringInput::from(&**fm),
        None,
    );
    let mut nesting = 0usize;
    let mut run = 0usize;
    let mut delimiters = Vec::new();
    let mut has_angle = false;
    for (count, token) in tokens.enumerate() {
        let start = (token.span.lo.0 - fm.start_pos.0) as usize;
        let end = (token.span.hi.0 - fm.start_pos.0) as usize;
        let text = fm.src.get(start..end).unwrap_or_default();
        match text {
            "(" | "[" | "{" | "${" => {
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
        // A wide dense literal has independent elements, not a deep expression
        // chain. Keep the old chain ceiling inside each element. Do not treat
        // commas inside generic angle syntax as element boundaries.
        has_angle |= text.starts_with('<') || text.starts_with('>');
        let boundary = matches!(text, ";" | "{" | "}");
        run = if boundary
            || (text == ","
                && delimiters.last().is_some_and(|(open, _)| *open == "[")
                && !has_angle)
        {
            0
        } else {
            run + 1
        };
        if boundary {
            has_angle = false;
        }
        if nesting > 64 || run > 256 || count > 65_536 {
            return Err(source.locate(
                CompilerError::Invalid("source complexity exceeds compiler limits".into()),
                token.span,
            ));
        }
    }

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
