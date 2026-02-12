mod error;
mod span;
mod token;

pub use error::LexError;
pub use span::{Span, SpannedToken};
pub use token::Token;

use logos::Logos;

/// Tokenize nanachi source code.
pub fn lex(input: &str) -> Result<Vec<SpannedToken>, LexError> {
    let mut tokens = Vec::new();
    let mut lexer = Token::lexer(input);

    while let Some(result) = lexer.next() {
        let span = Span {
            start: lexer.span().start,
            end: lexer.span().end,
        };
        let text = lexer.slice().to_string();

        match result {
            Ok(token) => tokens.push(SpannedToken { token, span, text }),
            Err(()) => return Err(LexError { span, text }),
        }
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_tokens(input: &str) -> Vec<Token> {
        lex(input).unwrap().into_iter().map(|t| t.token).collect()
    }

    #[test]
    fn hello_world() {
        let tokens = lex_tokens(r#"fn main() { println!("Hello, nanachi!"); }"#);
        assert_eq!(
            tokens,
            vec![
                Token::Fn,
                Token::Ident, // main
                Token::LParen,
                Token::RParen,
                Token::LBrace,
                Token::Ident, // println
                Token::Bang,
                Token::LParen,
                Token::StringLiteral,
                Token::RParen,
                Token::Semi,
                Token::RBrace,
            ]
        );
    }

    #[test]
    fn keywords_vs_idents() {
        let tokens = lex_tokens("fn foo let bar if else_branch");
        assert_eq!(
            tokens,
            vec![
                Token::Fn,
                Token::Ident, // foo
                Token::Let,
                Token::Ident, // bar
                Token::If,
                Token::Ident, // else_branch (not keyword — has suffix)
            ]
        );
    }

    #[test]
    fn numeric_literals() {
        let tokens = lex_tokens("42 3.14 1_000 1_0.2_5");
        assert_eq!(
            tokens,
            vec![
                Token::IntLiteral,
                Token::FloatLiteral,
                Token::IntLiteral,
                Token::FloatLiteral,
            ]
        );
    }

    #[test]
    fn string_and_char_literals() {
        let tokens = lex_tokens(r#""hello" "with \"escape\"" 'a' '\n'"#);
        assert_eq!(
            tokens,
            vec![
                Token::StringLiteral,
                Token::StringLiteral,
                Token::CharLiteral,
                Token::CharLiteral,
            ]
        );
    }

    #[test]
    fn optional_chaining_vs_question_dot() {
        let tokens = lex_tokens("value?.field");
        assert_eq!(tokens, vec![Token::Ident, Token::QuestionDot, Token::Ident]);
    }

    #[test]
    fn null_coalescing_vs_question() {
        let tokens = lex_tokens("value ?? 0");
        assert_eq!(
            tokens,
            vec![Token::Ident, Token::QuestionQuestion, Token::IntLiteral]
        );
    }

    #[test]
    fn question_alone() {
        let tokens = lex_tokens("User?");
        assert_eq!(tokens, vec![Token::Ident, Token::Question]);
    }

    #[test]
    fn compound_operators() {
        let tokens = lex_tokens("+= -= *= /= %= &= |= ^= <<= >>=");
        assert_eq!(
            tokens,
            vec![
                Token::PlusEq,
                Token::MinusEq,
                Token::StarEq,
                Token::SlashEq,
                Token::PercentEq,
                Token::AmpEq,
                Token::PipeEq,
                Token::CaretEq,
                Token::ShlEq,
                Token::ShrEq,
            ]
        );
    }

    #[test]
    fn arrow_and_fat_arrow() {
        let tokens = lex_tokens("-> =>");
        assert_eq!(tokens, vec![Token::Arrow, Token::FatArrow]);
    }

    #[test]
    fn ranges() {
        let tokens = lex_tokens("0..10 0..=9");
        assert_eq!(
            tokens,
            vec![
                Token::IntLiteral,
                Token::DotDot,
                Token::IntLiteral,
                Token::IntLiteral,
                Token::DotDotEq,
                Token::IntLiteral,
            ]
        );
    }

    #[test]
    fn self_keyword() {
        let tokens = lex_tokens("self.name Self::new");
        assert_eq!(
            tokens,
            vec![
                Token::SelfLower,
                Token::Dot,
                Token::Ident,
                Token::SelfUpper,
                Token::ColonColon,
                Token::Ident,
            ]
        );
    }

    #[test]
    fn line_comment_skipped() {
        let tokens = lex_tokens("let x = 5; // this is a comment\nlet y = 10;");
        assert_eq!(
            tokens,
            vec![
                Token::Let,
                Token::Ident,
                Token::Eq,
                Token::IntLiteral,
                Token::Semi,
                Token::Let,
                Token::Ident,
                Token::Eq,
                Token::IntLiteral,
                Token::Semi,
            ]
        );
    }

    #[test]
    fn span_tracking() {
        let tokens = lex("let x = 5;").unwrap();
        assert_eq!(tokens[0].span, Span { start: 0, end: 3 });
        assert_eq!(tokens[0].text, "let");
        assert_eq!(tokens[1].span, Span { start: 4, end: 5 });
        assert_eq!(tokens[1].text, "x");
    }

    #[test]
    fn shr_token() {
        let tokens = lex_tokens("Vec<Vec<i32>>");
        assert_eq!(
            tokens,
            vec![
                Token::Ident, // Vec
                Token::Lt,
                Token::Ident, // Vec
                Token::Lt,
                Token::Ident, // i32
                Token::Shr,   // >> (parser splits later)
            ]
        );
    }

    #[test]
    fn error_on_unknown_char() {
        let result = lex("let x = @;");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.text, "@");
    }

    #[test]
    fn full_function() {
        let input = r#"
            fn greet(name: String) {
                println!("Hello, {}", name);
            }
        "#;
        let tokens = lex_tokens(input);
        assert_eq!(
            tokens,
            vec![
                Token::Fn,
                Token::Ident, // greet
                Token::LParen,
                Token::Ident, // name
                Token::Colon,
                Token::Ident, // String
                Token::RParen,
                Token::LBrace,
                Token::Ident, // println
                Token::Bang,
                Token::LParen,
                Token::StringLiteral,
                Token::Comma,
                Token::Ident, // name
                Token::RParen,
                Token::Semi,
                Token::RBrace,
            ]
        );
    }
}
