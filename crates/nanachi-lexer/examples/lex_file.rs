use std::fmt::Write as _;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: cargo run -p nanachi-lexer --example lex_file <file.nanachi>");
        std::process::exit(1);
    }

    let path = std::path::PathBuf::from(&args[1]);
    let source = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {e}", path.display());
        std::process::exit(1);
    });

    match nanachi_lexer::lex(&source) {
        Ok(tokens) => {
            let mut out = String::new();
            writeln!(out, "=== Tokens: {} ===", path.display()).unwrap();
            for (i, tok) in tokens.iter().enumerate() {
                writeln!(
                    out,
                    "  [{i:3}] {:>4}..{:<4}  {:<20} {:?}",
                    tok.span.start,
                    tok.span.end,
                    format!("{:?}", tok.token),
                    tok.text,
                )
                .unwrap();
            }
            writeln!(out, "=== {} tokens ===", tokens.len()).unwrap();

            let out_path = path.with_extension("tokens");
            std::fs::write(&out_path, &out).unwrap();
            println!("{}", out_path.display());
        }
        Err(e) => {
            eprintln!(
                "Lex error at {}..{}: {:?}",
                e.span.start, e.span.end, e.text
            );
            std::process::exit(1);
        }
    }
}
