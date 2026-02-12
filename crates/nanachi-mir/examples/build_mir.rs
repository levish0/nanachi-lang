use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = args.get(1).map(String::as_str).unwrap_or("examples");

    let inputs = match collect_inputs(input) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(1);
        }
    };

    let mut failed = false;
    for path in inputs {
        match process_file(&path) {
            Ok(out_path) => println!("{}", out_path.display()),
            Err(msg) => {
                eprintln!("{msg}");
                failed = true;
            }
        }
    }

    if failed {
        std::process::exit(1);
    }
}

fn process_file(path: &Path) -> Result<PathBuf, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("Error reading {}: {e}", path.display()))?;

    let tokens = nanachi_lexer::lex(&source).map_err(|e| {
        format!(
            "Lex error in {} at {}..{}: {:?}",
            path.display(),
            e.span.start,
            e.span.end,
            e.text
        )
    })?;

    let ast = nanachi_parser::parse(&tokens).map_err(|e| {
        format!(
            "Parse error in {} at {}..{}: {}",
            path.display(),
            e.span.start,
            e.span.end,
            e.message
        )
    })?;

    let hir = nanachi_hir::lower(&ast)
        .map_err(|e| format!("HIR lower error in {}: {e}", path.display()))?;
    let mir = nanachi_mir::build(&hir)
        .map_err(|e| format!("MIR build error in {}: {e}", path.display()))?;

    let out = format!("=== MIR: {} ===\n{mir:#?}\n", path.display());
    let out_path = path.with_extension("mir");
    std::fs::write(&out_path, &out)
        .map_err(|e| format!("Error writing {}: {e}", out_path.display()))?;
    Ok(out_path)
}

fn collect_inputs(input: &str) -> Result<Vec<PathBuf>, String> {
    let path = PathBuf::from(input);
    if path.is_file() {
        return Ok(vec![path]);
    }
    if path.is_dir() {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&path)
            .map_err(|e| format!("Error reading {}: {e}", path.display()))?
        {
            let entry = entry.map_err(|e| format!("Error reading dir entry: {e}"))?;
            let file_path = entry.path();
            if file_path.is_file() && is_nanachi_source(&file_path) {
                files.push(file_path);
            }
        }
        files.sort();
        if files.is_empty() {
            return Err(format!(
                "No .nanachi/.nana files found in {}",
                path.display()
            ));
        }
        return Ok(files);
    }
    Err(format!("Path not found: {}", path.display()))
}

fn is_nanachi_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("nanachi") | Some("nana")
    )
}
