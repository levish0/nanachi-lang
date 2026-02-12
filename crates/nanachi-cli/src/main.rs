use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

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
                    Ok(out) => println!("{}", out.rs_path.display()),
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
            let mut entry_override: Option<String> = None;
            let mut program_args = Vec::new();
            let mut after_sep = false;
            while let Some(arg) = args.next() {
                if after_sep {
                    program_args.push(arg);
                    continue;
                }
                match arg.as_str() {
                    "--entry" => {
                        let value = args.next().ok_or_else(|| {
                            "Missing function name after --entry".to_string()
                        })?;
                        entry_override = Some(value);
                    }
                    "--" => {
                        after_sep = true;
                    }
                    _ => {
                        program_args.push(arg);
                    }
                }
            }

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

            let build = build_file(&path)?;
            println!("{}", build.rs_path.display());
            run_with_cargo(
                &path,
                &build.rs_path,
                &program_args,
                &build.top_level_functions,
                entry_override.as_deref(),
            )
        }
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(ExitCode::SUCCESS)
        }
        _ => Err(format!(
            "Unknown command: {cmd}\n\nUsage:\n  nanachi build <file.nanachi|dir>\n  nanachi run <file.nanachi|file.nana> [--entry <fn_name>] [-- <args...>]"
        )),
    }
}

fn print_usage() {
    println!("nanachi-lang CLI");
    println!();
    println!("Usage:");
    println!("  nanachi build <file.nanachi|dir>");
    println!("  nanachi run <file.nanachi|file.nana> [--entry <fn_name>] [-- <args...>]");
}

struct TopLevelFunction {
    name: String,
    param_count: usize,
    is_async: bool,
}

struct BuildOutput {
    rs_path: PathBuf,
    top_level_functions: Vec<TopLevelFunction>,
}

fn build_file(path: &Path) -> Result<BuildOutput, String> {
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

    let hir =
        nanachi_hir::lower(&ast).map_err(|e| format!("HIR lower error in {}: {e}", path.display()))?;
    let top_level_functions = collect_top_level_functions(&hir);
    let mir = nanachi_mir::build(&hir)
        .map_err(|e| format!("MIR build error in {}: {e}", path.display()))?;
    let analysis = nanachi_analyzer::analyze(&mir, &hir);
    let rust = nanachi_codegen::generate(&hir, &analysis)
        .map_err(|e| format!("Codegen error in {}: {e}", path.display()))?;

    let out_path = path.with_extension("rs");
    std::fs::write(&out_path, rust)
        .map_err(|e| format!("Error writing {}: {e}", out_path.display()))?;
    Ok(BuildOutput {
        rs_path: out_path,
        top_level_functions,
    })
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

fn run_with_cargo(
    source_path: &Path,
    rs_path: &Path,
    program_args: &[String],
    top_level_functions: &[TopLevelFunction],
    entry_override: Option<&str>,
) -> Result<ExitCode, String> {
    let mut rs_code =
        std::fs::read_to_string(rs_path).map_err(|e| format!("Error reading {}: {e}", rs_path.display()))?;

    let wrapper_entry = select_wrapper_entry(top_level_functions, entry_override)?;
    let wrapper_needs_tokio = wrapper_entry.as_ref().map(|e| e.is_async).unwrap_or(false);
    if let Some(entry) = wrapper_entry {
        rs_code.push_str("\n\n");
        if entry.is_async {
            rs_code.push_str("#[tokio::main]\n");
            rs_code.push_str("async fn main() {\n");
            rs_code.push_str("    let _ = ");
            rs_code.push_str(&entry.name);
            rs_code.push_str("().await;\n");
            rs_code.push_str("}\n");
        } else {
            rs_code.push_str("fn main() {\n");
            rs_code.push_str("    let _ = ");
            rs_code.push_str(&entry.name);
            rs_code.push_str("();\n");
            rs_code.push_str("}\n");
        }
    }

    let deps = infer_dependencies(&rs_code, wrapper_needs_tokio);
    let run_dir = runner_project_dir(source_path);
    let src_dir = run_dir.join("src");
    std::fs::create_dir_all(&src_dir)
        .map_err(|e| format!("Failed to create {}: {e}", src_dir.display()))?;

    let cargo_toml = render_runner_cargo_toml(source_path, deps);
    std::fs::write(run_dir.join("Cargo.toml"), cargo_toml)
        .map_err(|e| format!("Failed to write runner Cargo.toml: {e}"))?;
    std::fs::write(src_dir.join("main.rs"), rs_code)
        .map_err(|e| format!("Failed to write runner main.rs: {e}"))?;

    let mut cmd = Command::new("cargo");
    cmd.arg("run");
    if !program_args.is_empty() {
        cmd.arg("--");
        cmd.args(program_args);
    }
    let status = cmd
        .current_dir(&run_dir)
        .status()
        .map_err(|e| format!("Failed to run cargo in {}: {e}", run_dir.display()))?;

    if let Some(code) = status.code() {
        match u8::try_from(code) {
            Ok(v) => Ok(ExitCode::from(v)),
            Err(_) => Ok(ExitCode::from(1)),
        }
    } else {
        Ok(ExitCode::from(1))
    }
}

fn infer_dependencies(rs_code: &str, force_tokio: bool) -> Vec<&'static str> {
    let mut deps = Vec::new();
    if force_tokio || rs_code.contains("tokio::") || rs_code.contains("#[tokio::main]") {
        deps.push("tokio");
    }
    if rs_code.contains("reqwest::") {
        deps.push("reqwest");
    }
    deps.sort();
    deps.dedup();
    deps
}

fn render_runner_cargo_toml(source_path: &Path, deps: Vec<&str>) -> String {
    let pkg_name = runner_package_name(source_path);
    let mut out = String::new();
    out.push_str("[package]\n");
    out.push_str(&format!("name = \"{pkg_name}\"\n"));
    out.push_str("version = \"0.1.0\"\n");
    out.push_str("edition = \"2024\"\n\n");
    out.push_str("[dependencies]\n");
    for dep in deps {
        match dep {
            "tokio" => out.push_str("tokio = { version = \"1\", features = [\"full\"] }\n"),
            "reqwest" => out.push_str("reqwest = \"0.12\"\n"),
            _ => {}
        }
    }
    out
}

fn runner_project_dir(source_path: &Path) -> PathBuf {
    let canonical = source_path
        .canonicalize()
        .unwrap_or_else(|_| source_path.to_path_buf());
    let mut hasher = DefaultHasher::new();
    canonical.to_string_lossy().hash(&mut hasher);
    let h = hasher.finish();
    std::env::temp_dir()
        .join("nanachi-lang-runner")
        .join(format!("run-{h:016x}"))
}

fn runner_package_name(source_path: &Path) -> String {
    let stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("app");
    let mut out = String::new();
    out.push_str("nanachi-run-");
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('-');
        }
    }
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches('-').to_string()
}

fn collect_top_level_functions(hir: &nanachi_hir::HirProgram) -> Vec<TopLevelFunction> {
    let mut out = Vec::new();
    for item in &hir.items {
        if let nanachi_hir::HirItemKind::Function(func) = &item.kind {
            out.push(TopLevelFunction {
                name: func.name.clone(),
                param_count: func.params.len(),
                is_async: func.is_async,
            });
        }
    }
    out
}

fn select_wrapper_entry<'a>(
    functions: &'a [TopLevelFunction],
    entry_override: Option<&str>,
) -> Result<Option<&'a TopLevelFunction>, String> {
    if let Some(main_fn) = functions.iter().find(|f| f.name == "main") {
        if main_fn.param_count == 0 {
            return Ok(None);
        }
        return Err("Found `main` but it has parameters. `main` must be zero-arg.".to_string());
    }

    if let Some(name) = entry_override {
        let Some(entry) = functions.iter().find(|f| f.name == name) else {
            let names = functions
                .iter()
                .map(|f| f.name.clone())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "Entry function `{name}` not found. Available top-level functions: [{names}]"
            ));
        };
        if entry.param_count != 0 {
            return Err(format!(
                "Entry function `{name}` has {} parameter(s). `run` requires a zero-arg entry.",
                entry.param_count
            ));
        }
        return Ok(Some(entry));
    }

    let zero_arg = functions
        .iter()
        .filter(|f| f.param_count == 0)
        .collect::<Vec<_>>();
    match zero_arg.len() {
        0 => Err(
            "No runnable entry function found. Add `fn main()` or pass `--entry <zero-arg-fn>`."
                .to_string(),
        ),
        1 => Ok(Some(zero_arg[0])),
        _ => {
            let names = zero_arg
                .iter()
                .map(|f| f.name.clone())
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "Multiple zero-arg functions found: [{names}]. Use `--entry <fn_name>`."
            ))
        }
    }
}
