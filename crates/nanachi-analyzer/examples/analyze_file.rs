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

    let result = nanachi_analyzer::analyze(&mir, &hir);

    let mut out = format!("=== Analysis: {} ===\n\n", path.display());
    for (key, analysis) in &result.functions {
        out.push_str(&format!(
            "--- {} ---\n",
            if let Some(owner) = &key.owner {
                format!("{owner}::{}", key.name)
            } else {
                key.name.clone()
            }
        ));

        // Mutable vars
        if !analysis.mutable_vars.is_empty() {
            let mut vars: Vec<_> = analysis.mutable_vars.iter().collect();
            vars.sort();
            out.push_str(&format!("  mutable_vars: {:?}\n", vars));
        }

        // Self sig
        if let Some(sig) = &analysis.self_sig {
            out.push_str(&format!("  self_sig: {:?}\n", sig));
        }

        // Param sigs
        if !analysis.param_sigs.is_empty() {
            let mut params: Vec<_> = analysis.param_sigs.iter().collect();
            params.sort_by_key(|(k, _)| (*k).clone());
            for (name, sig) in params {
                out.push_str(&format!("  param {name}: {:?}\n", sig));
            }
        }

        // Call sites
        if !analysis.call_sites.is_empty() {
            out.push_str(&format!(
                "  call_sites: {} total\n",
                analysis.call_sites.len()
            ));
            for (span, info) in &analysis.call_sites {
                out.push_str(&format!(
                    "    @{}..{}: fallible={}, actions={:?}\n",
                    span.start, span.end, info.is_fallible, info.arg_actions
                ));
            }
        }

        // Error info
        if let Some(info) = &analysis.error_info {
            out.push_str(&format!(
                "  error_info: needs_wrap={}, types={}, enum={:?}\n",
                info.needs_result_wrap,
                info.error_types.len(),
                info.error_enum_name
            ));
        }

        out.push('\n');
    }

    // Warnings
    if !result.warnings.is_empty() {
        out.push_str("--- Warnings ---\n");
        for w in &result.warnings {
            out.push_str(&format!(
                "  @{}..{}: {}\n",
                w.span.start, w.span.end, w.message
            ));
        }
    }

    let out_path = path.with_extension("analysis");
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
