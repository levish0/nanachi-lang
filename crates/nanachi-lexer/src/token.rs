use logos::Logos;

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\n\r]+")]
#[logos(skip(r"//[^\n]*", allow_greedy = true))]
pub enum Token {
    // ── Keywords ──────────────────────────────────────────────
    #[token("fn")]
    Fn,
    #[token("let")]
    Let,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("while")]
    While,
    #[token("for")]
    For,
    #[token("in")]
    In,
    #[token("loop")]
    Loop,
    #[token("return")]
    Return,
    #[token("struct")]
    Struct,
    #[token("enum")]
    Enum,
    #[token("trait")]
    Trait,
    #[token("impl")]
    Impl,
    #[token("match")]
    Match,
    #[token("async")]
    Async,
    #[token("await")]
    Await,
    #[token("rust")]
    Rust,
    #[token("use")]
    Use,
    #[token("mod")]
    Mod,
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("pub")]
    Pub,
    #[token("self")]
    SelfLower,
    #[token("Self")]
    SelfUpper,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("as")]
    As,
    #[token("where")]
    Where,

    // ── Literals ──────────────────────────────────────────────
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*", priority = 3)]
    FloatLiteral,
    #[regex(r"[0-9][0-9_]*", priority = 2)]
    IntLiteral,
    #[regex(r#""([^"\\]|\\.)*""#)]
    StringLiteral,
    #[regex(r"'([^'\\]|\\.)'")]
    CharLiteral,

    // ── Identifier ────────────────────────────────────────────
    // Keywords take priority over this via #[token] vs #[regex].
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,

    // ── 3-char operators ──────────────────────────────────────
    #[token("<<=")]
    ShlEq,
    #[token(">>=")]
    ShrEq,
    #[token("..=")]
    DotDotEq,

    // ── 2-char operators ──────────────────────────────────────
    #[token("?.")]
    QuestionDot,
    #[token("??")]
    QuestionQuestion,
    #[token("&&")]
    AmpAmp,
    #[token("||")]
    PipePipe,
    #[token("<<")]
    Shl,
    #[token(">>")]
    Shr,
    #[token("==")]
    EqEq,
    #[token("!=")]
    BangEq,
    #[token("<=")]
    LtEq,
    #[token(">=")]
    GtEq,
    #[token("->")]
    Arrow,
    #[token("=>")]
    FatArrow,
    #[token("::")]
    ColonColon,
    #[token("..")]
    DotDot,
    #[token("+=")]
    PlusEq,
    #[token("-=")]
    MinusEq,
    #[token("*=")]
    StarEq,
    #[token("/=")]
    SlashEq,
    #[token("%=")]
    PercentEq,
    #[token("&=")]
    AmpEq,
    #[token("|=")]
    PipeEq,
    #[token("^=")]
    CaretEq,

    // ── 1-char operators ──────────────────────────────────────
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("&")]
    Amp,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,
    #[token("=")]
    Eq,
    #[token("!")]
    Bang,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token(".")]
    Dot,
    #[token("?")]
    Question,
    #[token(",")]
    Comma,
    #[token(":")]
    Colon,
    #[token(";")]
    Semi,
    #[token("#")]
    Hash,

    // ── Delimiters ────────────────────────────────────────────
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
}
