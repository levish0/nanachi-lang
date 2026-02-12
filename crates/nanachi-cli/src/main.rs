use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else {
        print_usage();
        return Ok(ExitCode::SUCCESS);
    };

    match cmd.as_str() {
        "build" => {
            let input = args.next().unwrap_or_else(|| "examples".to_string());
            let inputs = collect_inputs(&input)?;
            let mut failed = false;
            for path in inputs {
                match build_file(&path) {
                    Ok(out) => println!("{}", out.display()),
                    Err(msg) => {
                        eprintln!("{msg}");
                        failed = true;
                    }
                }
            }
            if failed {
                Ok(ExitCode::from(1))
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }
        "run" => {
            let Some(input) = args.next() else {
                return Err(
                    "Missing input file. Usage: nanachi run <file.nanachi|file.nana>".to_string(),
                );
            };

            let path = PathBuf::from(input);
            if !path.is_file() {
                return Err(format!("File not found: {}", path.display()));
            }
            if !is_nanachi_source(&path) {
                return Err(format!(
                    "Unsupported extension for {} (expected .nanachi or .nana)",
                    path.display()
                ));
            }

            let rs_path = build_file(&path)?;
            println!("{}", rs_path.display());

            let bin_path = compiled_bin_path(&path);
            let rustc_status = Command::new("rustc")
                .arg("--edition")
                .arg("2024")
                .arg(&rs_path)
                .arg("-o")
                .arg(&bin_path)
                .status()
                .map_err(|e| format!("Failed to run rustc: {e}"))?;

            if !rustc_status.success() {
                return Err(format!(
                    "rustc failed for {}. \
If this file requires external crates (tokio/reqwest/etc), use `nanachi build` and compile in a Cargo project.",
                    rs_path.display()
                ));
            }

            let run_status = Command::new(&bin_path)
                .status()
                .map_err(|e| format!("Failed to execute {}: {e}", bin_path.display()))?;

            if let Some(code) = run_status.code() {
                match u8::try_from(code) {
                    Ok(v) => Ok(ExitCode::from(v)),
                    Err(_) => Ok(ExitCode::from(1)),
                }
            } else {
                Ok(ExitCode::from(1))
            }
        }
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(ExitCode::SUCCESS)
        }
        _ => Err(format!(
            "Unknown command: {cmd}\n\nUsage:\n  nanachi build <file.nanachi|dir>\n  nanachi run <file.nanachi|file.nana>"
        )),
    }
}

fn print_usage() {
    println!("nanachi-lang CLI");
    println!();
    println!("Usage:");
    println!("  nanachi build <file.nanachi|dir>");
    println!("  nanachi run <file.nanachi|file.nana>");
}

fn build_file(path: &Path) -> Result<PathBuf, String> {
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
    let analysis = nanachi_analyzer::analyze(&mir, &hir);
    let rust = nanachi_codegen::generate(&hir, &analysis)
        .map_err(|e| format!("Codegen error in {}: {e}", path.display()))?;

    let out_path = path.with_extension("rs");
    std::fs::write(&out_path, rust)
        .map_err(|e| format!("Error writing {}: {e}", out_path.display()))?;
    Ok(out_path)
}

fn collect_inputs(input: &str) -> Result<Vec<PathBuf>, String> {
    let path = PathBuf::from(input);
    if path.is_file() {
        if !is_nanachi_source(&path) {
            return Err(format!(
                "Unsupported extension for {} (expected .nanachi or .nana)",
                path.display()
            ));
        }
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

fn compiled_bin_path(source_path: &Path) -> PathBuf {
    let stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("nanachi");
    if cfg!(windows) {
        source_path.with_file_name(format!("{stem}.nanachi.exe"))
    } else {
        source_path.with_file_name(format!("{stem}.nanachi.out"))
    }
}
