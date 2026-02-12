use super::*;
use nanachi_ast::expr::*;
use nanachi_ast::item::*;
use nanachi_ast::stmt::*;
use nanachi_lexer::lex;

fn parse_str(src: &str) -> Program {
    let tokens = lex(src).expect("lex failed");
    parse(&tokens).expect("parse failed")
}

#[test]
fn empty_program() {
    let prog = parse_str("");
    assert!(prog.items.is_empty());
}

#[test]
fn simple_fn_main() {
    let prog = parse_str("fn main() {}");
    assert_eq!(prog.items.len(), 1);
    match &prog.items[0].kind {
        ItemKind::Function(f) => {
            assert_eq!(f.name, "main");
            assert!(f.params.is_empty());
            assert!(f.return_ty.is_none());
            assert!(!f.is_async);
        }
        _ => panic!("expected function"),
    }
}

#[test]
fn fn_with_let_and_macro() {
    let prog = parse_str(
        r#"fn main() {
            let x: i32 = 42;
            println!("Hello from nanachi! x = {}", x);
        }"#,
    );
    assert_eq!(prog.items.len(), 1);
    match &prog.items[0].kind {
        ItemKind::Function(f) => {
            assert_eq!(f.name, "main");
            assert_eq!(f.body.stmts.len(), 2);
            // First stmt: let x: i32 = 42;
            match &f.body.stmts[0].kind {
                StmtKind::Let { ty, value, .. } => {
                    assert!(ty.is_some());
                    assert!(value.is_some());
                }
                _ => panic!("expected let statement"),
            }
            // Second stmt: println!(...)
            match &f.body.stmts[1].kind {
                StmtKind::Expr(expr) => match &expr.kind {
                    ExprKind::MacroCall { path, .. } => {
                        assert_eq!(path.segments, vec!["println"]);
                    }
                    _ => panic!("expected macro call"),
                },
                _ => panic!("expected expr statement"),
            }
        }
        _ => panic!("expected function"),
    }
}

#[test]
fn fn_with_return_type() {
    let prog = parse_str("fn add(a: i32, b: i32) -> i32 { a + b }");
    match &prog.items[0].kind {
        ItemKind::Function(f) => {
            assert_eq!(f.name, "add");
            assert_eq!(f.params.len(), 2);
            assert!(f.return_ty.is_some());
            assert!(f.body.tail_expr.is_some());
        }
        _ => panic!("expected function"),
    }
}

#[test]
fn struct_definition() {
    let prog = parse_str("struct Point { x: f64, y: f64 }");
    match &prog.items[0].kind {
        ItemKind::Struct(s) => {
            assert_eq!(s.name, "Point");
            assert_eq!(s.fields.len(), 2);
            assert_eq!(s.fields[0].name, "x");
            assert_eq!(s.fields[1].name, "y");
        }
        _ => panic!("expected struct"),
    }
}

#[test]
fn enum_definition() {
    let prog = parse_str("enum Color { Red, Green, Blue }");
    match &prog.items[0].kind {
        ItemKind::Enum(e) => {
            assert_eq!(e.name, "Color");
            assert_eq!(e.variants.len(), 3);
        }
        _ => panic!("expected enum"),
    }
}

#[test]
fn if_else_expression() {
    let prog = parse_str("fn f() { if x { 1 } else { 2 } }");
    match &prog.items[0].kind {
        ItemKind::Function(f) => {
            assert!(!f.body.stmts.is_empty() || f.body.tail_expr.is_some());
        }
        _ => panic!("expected function"),
    }
}

#[test]
fn while_loop() {
    let prog = parse_str("fn f() { while x { y; } }");
    match &prog.items[0].kind {
        ItemKind::Function(f) => {
            assert!(!f.body.stmts.is_empty());
            match &f.body.stmts[0].kind {
                StmtKind::While { .. } => {}
                _ => panic!("expected while"),
            }
        }
        _ => panic!("expected function"),
    }
}

#[test]
fn for_loop() {
    let prog = parse_str("fn f() { for i in items { process; } }");
    match &prog.items[0].kind {
        ItemKind::Function(f) => match &f.body.stmts[0].kind {
            StmtKind::For { .. } => {}
            _ => panic!("expected for"),
        },
        _ => panic!("expected function"),
    }
}

#[test]
fn optional_type() {
    let prog = parse_str("fn f(x: i32?) {}");
    match &prog.items[0].kind {
        ItemKind::Function(f) => match &f.params[0].kind {
            FnParamKind::Typed { ty, .. } => {
                assert!(matches!(ty.kind, nanachi_ast::types::TypeKind::Option(_)));
            }
            _ => panic!("expected typed param"),
        },
        _ => panic!("expected function"),
    }
}

#[test]
fn generic_type() {
    let prog = parse_str("fn f(v: Vec<i32>) {}");
    match &prog.items[0].kind {
        ItemKind::Function(f) => match &f.params[0].kind {
            FnParamKind::Typed { ty, .. } => match &ty.kind {
                nanachi_ast::types::TypeKind::Named { path, generics } => {
                    assert_eq!(path.segments, vec!["Vec"]);
                    assert_eq!(generics.len(), 1);
                }
                _ => panic!("expected named type"),
            },
            _ => panic!("expected typed param"),
        },
        _ => panic!("expected function"),
    }
}

#[test]
fn binary_expression() {
    let prog = parse_str("fn f() -> i32 { 1 + 2 * 3 }");
    match &prog.items[0].kind {
        ItemKind::Function(f) => {
            // Should parse as 1 + (2 * 3) due to precedence
            let tail = f.body.tail_expr.as_ref().unwrap();
            match &tail.kind {
                ExprKind::BinaryOp { op, .. } => {
                    assert_eq!(*op, BinOp::Add);
                }
                _ => panic!("expected binary op"),
            }
        }
        _ => panic!("expected function"),
    }
}

#[test]
fn use_item() {
    let prog = parse_str("use std::io;");
    match &prog.items[0].kind {
        ItemKind::Use(u) => match &u.tree {
            nanachi_ast::item::UseTree::Simple { path, alias } => {
                assert_eq!(path.segments, vec!["std", "io"]);
                assert!(alias.is_none());
            }
            _ => panic!("expected simple use"),
        },
        _ => panic!("expected use"),
    }
}

#[test]
fn trait_definition() {
    let prog = parse_str("trait Greet { fn greet(self); }");
    match &prog.items[0].kind {
        ItemKind::Trait(t) => {
            assert_eq!(t.name, "Greet");
            assert_eq!(t.methods.len(), 1);
            assert_eq!(t.methods[0].name, "greet");
        }
        _ => panic!("expected trait"),
    }
}

#[test]
fn impl_block() {
    let prog = parse_str("impl Point { fn new() -> Point { Point { x: 0, y: 0 } } }");
    match &prog.items[0].kind {
        ItemKind::Impl(i) => {
            assert!(i.trait_name.is_none());
            assert_eq!(i.methods.len(), 1);
            assert_eq!(i.methods[0].name, "new");
        }
        _ => panic!("expected impl"),
    }
}

// ── Type system tests ───────────────────────────────────────

#[test]
fn nested_generic_shr_split() {
    // `Vec<Vec<i32>>` — lexer emits `>>` as Shr, parser must split into `>` + `>`
    let prog = parse_str("fn f(v: Vec<Vec<i32>>) {}");
    match &prog.items[0].kind {
        ItemKind::Function(f) => match &f.params[0].kind {
            FnParamKind::Typed { ty, .. } => match &ty.kind {
                nanachi_ast::types::TypeKind::Named { path, generics } => {
                    assert_eq!(path.segments, vec!["Vec"]);
                    assert_eq!(generics.len(), 1);
                    // Inner: Vec<i32>
                    match &generics[0].kind {
                        nanachi_ast::types::TypeKind::Named {
                            path: inner_path,
                            generics: inner_generics,
                        } => {
                            assert_eq!(inner_path.segments, vec!["Vec"]);
                            assert_eq!(inner_generics.len(), 1);
                            assert!(matches!(
                                inner_generics[0].kind,
                                nanachi_ast::types::TypeKind::Primitive(
                                    nanachi_ast::types::PrimitiveType::I32
                                )
                            ));
                        }
                        _ => panic!("expected inner Vec<i32>"),
                    }
                }
                _ => panic!("expected named type"),
            },
            _ => panic!("expected typed param"),
        },
        _ => panic!("expected function"),
    }
}

#[test]
fn triple_nested_generic() {
    // `A<B<C<i32>>>` — three levels of `>` splitting
    let prog = parse_str("fn f(x: A<B<C<i32>>>) {}");
    match &prog.items[0].kind {
        ItemKind::Function(f) => match &f.params[0].kind {
            FnParamKind::Typed { ty, .. } => match &ty.kind {
                nanachi_ast::types::TypeKind::Named { path, generics } => {
                    assert_eq!(path.segments, vec!["A"]);
                    assert_eq!(generics.len(), 1);
                }
                _ => panic!("expected named type"),
            },
            _ => panic!("expected typed param"),
        },
        _ => panic!("expected function"),
    }
}

#[test]
fn multi_param_generic() {
    // `HashMap<String, Vec<i32>>`
    let prog = parse_str("fn f(m: HashMap<String, Vec<i32>>) {}");
    match &prog.items[0].kind {
        ItemKind::Function(f) => match &f.params[0].kind {
            FnParamKind::Typed { ty, .. } => match &ty.kind {
                nanachi_ast::types::TypeKind::Named { path, generics } => {
                    assert_eq!(path.segments, vec!["HashMap"]);
                    assert_eq!(generics.len(), 2);
                    // Second param: Vec<i32>
                    match &generics[1].kind {
                        nanachi_ast::types::TypeKind::Named {
                            path: inner,
                            generics: inner_g,
                        } => {
                            assert_eq!(inner.segments, vec!["Vec"]);
                            assert_eq!(inner_g.len(), 1);
                        }
                        _ => panic!("expected Vec<i32>"),
                    }
                }
                _ => panic!("expected named type"),
            },
            _ => panic!("expected typed param"),
        },
        _ => panic!("expected function"),
    }
}

#[test]
fn optional_generic() {
    // `Vec<i32>?` — Option wrapping a generic type
    let prog = parse_str("fn f(v: Vec<i32>?) {}");
    match &prog.items[0].kind {
        ItemKind::Function(f) => match &f.params[0].kind {
            FnParamKind::Typed { ty, .. } => match &ty.kind {
                nanachi_ast::types::TypeKind::Option(inner) => match &inner.kind {
                    nanachi_ast::types::TypeKind::Named { path, generics } => {
                        assert_eq!(path.segments, vec!["Vec"]);
                        assert_eq!(generics.len(), 1);
                    }
                    _ => panic!("expected Vec<i32> inside Option"),
                },
                _ => panic!("expected Option type"),
            },
            _ => panic!("expected typed param"),
        },
        _ => panic!("expected function"),
    }
}

#[test]
fn tuple_type() {
    let prog = parse_str("fn f(t: (i32, f64, bool)) {}");
    match &prog.items[0].kind {
        ItemKind::Function(f) => match &f.params[0].kind {
            FnParamKind::Typed { ty, .. } => match &ty.kind {
                nanachi_ast::types::TypeKind::Tuple(types) => {
                    assert_eq!(types.len(), 3);
                }
                _ => panic!("expected tuple type"),
            },
            _ => panic!("expected typed param"),
        },
        _ => panic!("expected function"),
    }
}

#[test]
fn unit_return_type() {
    let prog = parse_str("fn f() -> () {}");
    match &prog.items[0].kind {
        ItemKind::Function(f) => match &f.return_ty {
            Some(ty) => assert!(matches!(ty.kind, nanachi_ast::types::TypeKind::Unit)),
            None => panic!("expected unit return type"),
        },
        _ => panic!("expected function"),
    }
}

#[test]
fn path_type() {
    // `std::io::Error`
    let prog = parse_str("fn f(e: std::io::Error) {}");
    match &prog.items[0].kind {
        ItemKind::Function(f) => match &f.params[0].kind {
            FnParamKind::Typed { ty, .. } => match &ty.kind {
                nanachi_ast::types::TypeKind::Named { path, generics } => {
                    assert_eq!(path.segments, vec!["std", "io", "Error"]);
                    assert!(generics.is_empty());
                }
                _ => panic!("expected named type"),
            },
            _ => panic!("expected typed param"),
        },
        _ => panic!("expected function"),
    }
}

#[test]
fn nested_option_generic() {
    // `Option<Vec<i32>>?` shouldn't exist logically but tests deep nesting
    // Actually test: `Result<Vec<i32>, Error>` with nested `>`
    let prog = parse_str("fn f(r: Result<Vec<i32>, String>) {}");
    match &prog.items[0].kind {
        ItemKind::Function(f) => match &f.params[0].kind {
            FnParamKind::Typed { ty, .. } => match &ty.kind {
                nanachi_ast::types::TypeKind::Named { path, generics } => {
                    assert_eq!(path.segments, vec!["Result"]);
                    assert_eq!(generics.len(), 2);
                }
                _ => panic!("expected named type"),
            },
            _ => panic!("expected typed param"),
        },
        _ => panic!("expected function"),
    }
}

// ── Expression tests ────────────────────────────────────────

#[test]
fn method_chain() {
    let prog = parse_str("fn f() { x.foo().bar(); }");
    assert_eq!(prog.items.len(), 1);
}

#[test]
fn optional_chaining() {
    let prog = parse_str("fn f() { x?.field; }");
    assert_eq!(prog.items.len(), 1);
}

#[test]
fn null_coalescing() {
    let prog = parse_str("fn f() -> i32 { x ?? 0 }");
    match &prog.items[0].kind {
        ItemKind::Function(f) => {
            let tail = f.body.tail_expr.as_ref().unwrap();
            assert!(matches!(tail.kind, ExprKind::NullCoalesce { .. }));
        }
        _ => panic!("expected function"),
    }
}

#[test]
fn closure_expr() {
    let prog = parse_str("fn f() { |x: i32| x + 1 }");
    assert_eq!(prog.items.len(), 1);
}

#[test]
fn match_expression() {
    let prog = parse_str(
        r#"fn f() {
            match x {
                0 => 1,
                1 => 2,
                _ => 3,
            }
        }"#,
    );
    assert_eq!(prog.items.len(), 1);
}

#[test]
fn async_fn() {
    let prog = parse_str("async fn fetch() {}");
    match &prog.items[0].kind {
        ItemKind::Function(f) => {
            assert!(f.is_async);
            assert_eq!(f.name, "fetch");
        }
        _ => panic!("expected function"),
    }
}

#[test]
fn generic_fn() {
    let prog = parse_str("fn identity<T>(x: T) -> T { x }");
    match &prog.items[0].kind {
        ItemKind::Function(f) => {
            assert_eq!(f.generics.len(), 1);
            assert_eq!(f.generics[0].name, "T");
        }
        _ => panic!("expected function"),
    }
}

#[test]
fn impl_trait_for_type() {
    let prog = parse_str("impl Display for Point { fn fmt(self) {} }");
    match &prog.items[0].kind {
        ItemKind::Impl(i) => {
            assert!(i.trait_name.is_some());
            let trait_path = i.trait_name.as_ref().unwrap();
            assert_eq!(trait_path.segments, vec!["Display"]);
        }
        _ => panic!("expected impl"),
    }
}

#[test]
fn enum_with_tuple_variant() {
    let prog = parse_str("enum Shape { Circle(f64), Rect(f64, f64) }");
    match &prog.items[0].kind {
        ItemKind::Enum(e) => {
            assert_eq!(e.variants.len(), 2);
            assert!(matches!(
                e.variants[0].fields,
                nanachi_ast::item::VariantFields::Tuple(_)
            ));
        }
        _ => panic!("expected enum"),
    }
}

#[test]
fn pub_fn() {
    let prog = parse_str("pub fn api() {}");
    match &prog.items[0].kind {
        ItemKind::Function(f) => {
            assert_eq!(f.visibility, Visibility::Public);
        }
        _ => panic!("expected function"),
    }
}

// ── sketch.txt integration tests ─────────────────────────────

#[test]
fn sketch_hello_world() {
    let prog = parse_str(
        r#"
        fn main() {
            println!("Hello, nanachi!");
        }
    "#,
    );
    let f = expect_fn(&prog, 0, "main");
    assert!(f.params.is_empty());
    assert!(f.return_ty.is_none());
    // body: println!(...);
    assert_eq!(f.body.stmts.len(), 1);
    match &f.body.stmts[0].kind {
        StmtKind::Expr(e) => assert!(
            matches!(&e.kind, ExprKind::MacroCall { path, .. } if path.segments == ["println"])
        ),
        _ => panic!("expected macro call stmt"),
    }
}

#[test]
fn sketch_variables() {
    let prog = parse_str(
        r#"
        fn variables() {
            let x: i32 = 5;
            x = 10;
            let y: i32 = 3;
            println!("{} {}", x, y);
        }
    "#,
    );
    let f = expect_fn(&prog, 0, "variables");
    assert_eq!(f.body.stmts.len(), 4);
    // stmt 0: let x: i32 = 5;
    match &f.body.stmts[0].kind {
        StmtKind::Let { pattern, ty, value } => {
            assert!(
                matches!(&pattern.kind, nanachi_ast::pattern::PatternKind::Ident(n) if n == "x")
            );
            assert!(matches!(
                &ty.as_ref().unwrap().kind,
                nanachi_ast::types::TypeKind::Primitive(nanachi_ast::types::PrimitiveType::I32)
            ));
            assert!(
                matches!(&value.as_ref().unwrap().kind, ExprKind::Literal(Literal::Int(v)) if v == "5")
            );
        }
        _ => panic!("expected let"),
    }
    // stmt 1: x = 10;
    match &f.body.stmts[1].kind {
        StmtKind::Expr(e) => match &e.kind {
            ExprKind::Assign { target, value } => {
                assert!(matches!(&target.kind, ExprKind::Path(p) if p.segments == ["x"]));
                assert!(matches!(&value.kind, ExprKind::Literal(Literal::Int(v)) if v == "10"));
            }
            _ => panic!("expected assign"),
        },
        _ => panic!("expected expr stmt"),
    }
    // stmt 2: let y: i32 = 3;
    match &f.body.stmts[2].kind {
        StmtKind::Let { pattern, .. } => {
            assert!(
                matches!(&pattern.kind, nanachi_ast::pattern::PatternKind::Ident(n) if n == "y")
            );
        }
        _ => panic!("expected let"),
    }
    // stmt 3: println!(...)
    match &f.body.stmts[3].kind {
        StmtKind::Expr(e) => assert!(matches!(&e.kind, ExprKind::MacroCall { .. })),
        _ => panic!("expected macro call"),
    }
}

#[test]
fn sketch_ownership() {
    let prog = parse_str(
        r#"
        fn greet(name: String) {
            println!("Hello, {}", name);
        }

        fn push_name(list: Vec<String>, name: String) {
            list.push(name);
        }

        fn ownership_example() {
            let a: String = "hello";
            let b: String = "world";
            let list: Vec<String> = Vec::new();
            greet(a);
            greet(b);
            push_name(list, a);
            push_name(list, b);
            println!("{}", b);
        }
    "#,
    );
    assert_eq!(prog.items.len(), 3);
    // greet(name: String)
    let greet = expect_fn(&prog, 0, "greet");
    assert_eq!(greet.params.len(), 1);
    match &greet.params[0].kind {
        FnParamKind::Typed { name, ty } => {
            assert_eq!(name, "name");
            assert!(
                matches!(&ty.kind, nanachi_ast::types::TypeKind::Named { path, .. } if path.segments == ["String"])
            );
        }
        _ => panic!("expected typed param"),
    }
    // push_name(list: Vec<String>, name: String)
    let push = expect_fn(&prog, 1, "push_name");
    assert_eq!(push.params.len(), 2);
    match &push.params[0].kind {
        FnParamKind::Typed { name, ty } => {
            assert_eq!(name, "list");
            match &ty.kind {
                nanachi_ast::types::TypeKind::Named { path, generics } => {
                    assert_eq!(path.segments, vec!["Vec"]);
                    assert_eq!(generics.len(), 1);
                }
                _ => panic!("expected Vec<String>"),
            }
        }
        _ => panic!("expected typed param"),
    }
    // push_name body: list.push(name);
    assert_eq!(push.body.stmts.len(), 1);
    match &push.body.stmts[0].kind {
        StmtKind::Expr(e) => {
            assert!(matches!(&e.kind, ExprKind::MethodCall { method, .. } if method == "push"))
        }
        _ => panic!("expected method call stmt"),
    }
    // ownership_example: 8 stmts
    let owner = expect_fn(&prog, 2, "ownership_example");
    assert_eq!(owner.body.stmts.len(), 8);
}

#[test]
fn sketch_error_propagation() {
    let prog = parse_str(
        r#"
        fn read_config(path: String) -> String {
            let content: String = fs::read_to_string(path);
            content
        }

        fn process() {
            let config: String = read_config("config.toml");
            println!("{}", config);
        }
    "#,
    );
    assert_eq!(prog.items.len(), 2);
    // read_config: return type String, has tail expr
    let rc = expect_fn(&prog, 0, "read_config");
    assert!(
        matches!(&rc.return_ty.as_ref().unwrap().kind, nanachi_ast::types::TypeKind::Named { path, .. } if path.segments == ["String"])
    );
    assert_eq!(rc.body.stmts.len(), 1); // let content
    assert!(rc.body.tail_expr.is_some()); // content
    // body: let content = fs::read_to_string(path);
    match &rc.body.stmts[0].kind {
        StmtKind::Let { value, .. } => match &value.as_ref().unwrap().kind {
            ExprKind::FnCall { func, args } => {
                assert!(
                    matches!(&func.kind, ExprKind::Path(p) if p.segments == ["fs", "read_to_string"])
                );
                assert_eq!(args.len(), 1);
            }
            _ => panic!("expected fn call"),
        },
        _ => panic!("expected let"),
    }
    // process()
    let proc = expect_fn(&prog, 1, "process");
    assert_eq!(proc.body.stmts.len(), 2);
}

#[test]
fn sketch_option_return_type() {
    let prog = parse_str(
        r#"
        fn find_user(id: i32) -> User? {
            if id == 1 {
                Some(User { name: "nanachi", age: 3 })
            } else {
                None
            }
        }
    "#,
    );
    let f = expect_fn(&prog, 0, "find_user");
    // return type: User? = Option(Named "User")
    match &f.return_ty.as_ref().unwrap().kind {
        nanachi_ast::types::TypeKind::Option(inner) => {
            assert!(
                matches!(&inner.kind, nanachi_ast::types::TypeKind::Named { path, .. } if path.segments == ["User"])
            );
        }
        _ => panic!("expected Option type"),
    }
    // body: if expr (block-like, no semi needed)
    assert_eq!(f.body.stmts.len(), 1);
    match &f.body.stmts[0].kind {
        StmtKind::Expr(e) => match &e.kind {
            ExprKind::If {
                condition,
                then_block,
                else_expr,
            } => {
                // condition: id == 1
                assert!(matches!(
                    &condition.kind,
                    ExprKind::BinaryOp { op: BinOp::Eq, .. }
                ));
                // then: Some(User { name: "nanachi", age: 3 })
                let tail = then_block.tail_expr.as_ref().unwrap();
                assert!(matches!(&tail.kind, ExprKind::FnCall { func, args } if {
                    matches!(&func.kind, ExprKind::Path(p) if p.segments == ["Some"])
                    && args.len() == 1
                    && matches!(&args[0].kind, ExprKind::StructLiteral { path, fields } if path.segments == ["User"] && fields.len() == 2)
                }));
                // else: None
                assert!(else_expr.is_some());
            }
            _ => panic!("expected if"),
        },
        _ => panic!("expected expr stmt"),
    }
}

#[test]
fn sketch_option_usage() {
    let prog = parse_str(
        r#"
        fn option_example() {
            let user: User? = find_user(42);
            let name: String? = find_user(1)?.name;
            let age: i32 = find_user(1)?.age ?? 0;
        }
    "#,
    );
    let f = expect_fn(&prog, 0, "option_example");
    assert_eq!(f.body.stmts.len(), 3);
    // stmt 0: let user: User? = find_user(42);
    match &f.body.stmts[0].kind {
        StmtKind::Let { ty, value, .. } => {
            assert!(matches!(
                &ty.as_ref().unwrap().kind,
                nanachi_ast::types::TypeKind::Option(_)
            ));
            assert!(matches!(
                &value.as_ref().unwrap().kind,
                ExprKind::FnCall { .. }
            ));
        }
        _ => panic!("expected let"),
    }
    // stmt 1: let name: String? = find_user(1)?.name;
    match &f.body.stmts[1].kind {
        StmtKind::Let { ty, value, .. } => {
            assert!(matches!(
                &ty.as_ref().unwrap().kind,
                nanachi_ast::types::TypeKind::Option(_)
            ));
            match &value.as_ref().unwrap().kind {
                ExprKind::OptionalChain { receiver, access } => {
                    assert!(matches!(&receiver.kind, ExprKind::FnCall { .. }));
                    assert!(matches!(access, OptionalAccess::Field(f) if f == "name"));
                }
                _ => panic!("expected optional chain"),
            }
        }
        _ => panic!("expected let"),
    }
    // stmt 2: let age: i32 = find_user(1)?.age ?? 0;
    match &f.body.stmts[2].kind {
        StmtKind::Let { ty, value, .. } => {
            assert!(matches!(
                &ty.as_ref().unwrap().kind,
                nanachi_ast::types::TypeKind::Primitive(nanachi_ast::types::PrimitiveType::I32)
            ));
            match &value.as_ref().unwrap().kind {
                ExprKind::NullCoalesce { expr, default } => {
                    assert!(
                        matches!(&expr.kind, ExprKind::OptionalChain { access: OptionalAccess::Field(f), .. } if f == "age")
                    );
                    assert!(
                        matches!(&default.kind, ExprKind::Literal(Literal::Int(v)) if v == "0")
                    );
                }
                _ => panic!("expected null coalesce"),
            }
        }
        _ => panic!("expected let"),
    }
}

#[test]
fn sketch_struct_impl() {
    let prog = parse_str(
        r#"
        struct User {
            name: String,
            age: i32,
        }

        impl User {
            fn new(name: String, age: i32) -> User {
                User { name, age }
            }

            fn greet(self) {
                println!("Hi, I'm {} ({})", self.name, self.age);
            }

            fn grow(self) {
                self.age = self.age + 1;
            }
        }
    "#,
    );
    assert_eq!(prog.items.len(), 2);
    // struct User { name: String, age: i32 }
    match &prog.items[0].kind {
        ItemKind::Struct(s) => {
            assert_eq!(s.name, "User");
            assert_eq!(s.fields.len(), 2);
            assert_eq!(s.fields[0].name, "name");
            assert_eq!(s.fields[1].name, "age");
        }
        _ => panic!("expected struct"),
    }
    // impl User { new, greet, grow }
    match &prog.items[1].kind {
        ItemKind::Impl(i) => {
            assert!(i.trait_name.is_none());
            assert_eq!(i.methods.len(), 3);
            assert_eq!(i.methods[0].name, "new");
            assert_eq!(i.methods[1].name, "greet");
            assert_eq!(i.methods[2].name, "grow");
            // new: 2 params, return User, body has struct literal
            assert_eq!(i.methods[0].params.len(), 2);
            assert!(i.methods[0].return_ty.is_some());
            let new_tail = i.methods[0].body.tail_expr.as_ref().unwrap();
            assert!(
                matches!(&new_tail.kind, ExprKind::StructLiteral { path, fields } if path.segments == ["User"] && fields.len() == 2)
            );
            // greet: self param
            assert_eq!(i.methods[1].params.len(), 1);
            assert!(matches!(
                &i.methods[1].params[0].kind,
                FnParamKind::SelfParam
            ));
            // grow: self param, body has assignment self.age = self.age + 1
            assert_eq!(i.methods[2].params.len(), 1);
            match &i.methods[2].body.stmts[0].kind {
                StmtKind::Expr(e) => assert!(matches!(&e.kind, ExprKind::Assign { .. })),
                _ => panic!("expected assignment"),
            }
        }
        _ => panic!("expected impl"),
    }
}

#[test]
fn sketch_enum_match() {
    let prog = parse_str(
        r#"
        enum Shape {
            Circle { radius: f64 },
            Rectangle { width: f64, height: f64 },
        }

        fn area(shape: Shape) -> f64 {
            match shape {
                Shape::Circle { radius } => 3.14159 * radius * radius,
                Shape::Rectangle { width, height } => width * height,
            }
        }
    "#,
    );
    assert_eq!(prog.items.len(), 2);
    // enum Shape
    match &prog.items[0].kind {
        ItemKind::Enum(e) => {
            assert_eq!(e.name, "Shape");
            assert_eq!(e.variants.len(), 2);
            assert_eq!(e.variants[0].name, "Circle");
            assert_eq!(e.variants[1].name, "Rectangle");
            // Circle has 1 field, Rectangle has 2
            match &e.variants[0].fields {
                nanachi_ast::item::VariantFields::Struct(fs) => assert_eq!(fs.len(), 1),
                _ => panic!("expected struct variant"),
            }
            match &e.variants[1].fields {
                nanachi_ast::item::VariantFields::Struct(fs) => assert_eq!(fs.len(), 2),
                _ => panic!("expected struct variant"),
            }
        }
        _ => panic!("expected enum"),
    }
    // fn area: match with 2 arms
    let area = expect_fn(&prog, 1, "area");
    assert!(matches!(
        &area.return_ty.as_ref().unwrap().kind,
        nanachi_ast::types::TypeKind::Primitive(nanachi_ast::types::PrimitiveType::F64)
    ));
    match &area.body.stmts[0].kind {
        StmtKind::Expr(e) => match &e.kind {
            ExprKind::Match { expr, arms } => {
                assert!(matches!(&expr.kind, ExprKind::Path(p) if p.segments == ["shape"]));
                assert_eq!(arms.len(), 2);
                // arm 0: Shape::Circle { radius } => 3.14159 * radius * radius
                match &arms[0].pattern.kind {
                    nanachi_ast::pattern::PatternKind::Struct { path, fields } => {
                        assert_eq!(path.segments, vec!["Shape", "Circle"]);
                        assert_eq!(fields.len(), 1);
                        assert_eq!(fields[0].name, "radius");
                    }
                    _ => panic!("expected struct pattern"),
                }
                // body: Mul(Mul(3.14159, radius), radius)
                assert!(matches!(
                    &arms[0].body.kind,
                    ExprKind::BinaryOp { op: BinOp::Mul, .. }
                ));
                // arm 1: Shape::Rectangle { width, height } => width * height
                match &arms[1].pattern.kind {
                    nanachi_ast::pattern::PatternKind::Struct { path, fields } => {
                        assert_eq!(path.segments, vec!["Shape", "Rectangle"]);
                        assert_eq!(fields.len(), 2);
                    }
                    _ => panic!("expected struct pattern"),
                }
                assert!(matches!(
                    &arms[1].body.kind,
                    ExprKind::BinaryOp { op: BinOp::Mul, .. }
                ));
            }
            _ => panic!("expected match"),
        },
        _ => panic!("expected expr stmt"),
    }
}

#[test]
fn sketch_trait_generic() {
    let prog = parse_str(
        r#"
        trait Printable {
            fn to_string(self) -> String;
        }

        impl Printable for User {
            fn to_string(self) -> String {
                format!("{} (age {})", self.name, self.age)
            }
        }

        fn print_it<T: Printable>(item: T) {
            println!("{}", item.to_string());
        }
    "#,
    );
    assert_eq!(prog.items.len(), 3);
    // trait Printable { fn to_string(self) -> String; }
    match &prog.items[0].kind {
        ItemKind::Trait(t) => {
            assert_eq!(t.name, "Printable");
            assert_eq!(t.methods.len(), 1);
            assert_eq!(t.methods[0].name, "to_string");
            assert!(matches!(
                &t.methods[0].params[0].kind,
                FnParamKind::SelfParam
            ));
        }
        _ => panic!("expected trait"),
    }
    // impl Printable for User
    match &prog.items[1].kind {
        ItemKind::Impl(i) => {
            assert!(matches!(&i.trait_name, Some(p) if p.segments == ["Printable"]));
            assert_eq!(i.methods.len(), 1);
            assert_eq!(i.methods[0].name, "to_string");
        }
        _ => panic!("expected impl"),
    }
    // fn print_it<T: Printable>(item: T)
    let f = expect_fn(&prog, 2, "print_it");
    assert_eq!(f.generics.len(), 1);
    assert_eq!(f.generics[0].name, "T");
    assert_eq!(f.generics[0].bounds.len(), 1);
    assert_eq!(f.params.len(), 1);
}

#[test]
fn sketch_control_flow() {
    let prog = parse_str(
        r#"
        fn control_flow() {
            let x: i32 = 10;
            let abs: i32 = if x > 0 { x } else { -x };
            let numbers: Vec<i32> = vec![1, 2, 3, 4, 5];
            for n: i32 in numbers {
                println!("{}", n);
            }
            println!("len: {}", numbers.len());
            let count: i32 = 0;
            while count < 10 {
                count = count + 1;
            }
        }
    "#,
    );
    let f = expect_fn(&prog, 0, "control_flow");
    // 7 stmts: let x, let abs, let numbers, for, println, let count, while
    assert_eq!(f.body.stmts.len(), 7);
    // stmt 1: let abs = if x > 0 { x } else { -x };
    match &f.body.stmts[1].kind {
        StmtKind::Let { value, .. } => {
            assert!(matches!(
                &value.as_ref().unwrap().kind,
                ExprKind::If {
                    else_expr: Some(_),
                    ..
                }
            ));
        }
        _ => panic!("expected let"),
    }
    // stmt 2: let numbers: Vec<i32> = vec![1, 2, 3, 4, 5];
    match &f.body.stmts[2].kind {
        StmtKind::Let { ty, value, .. } => {
            assert!(
                matches!(&ty.as_ref().unwrap().kind, nanachi_ast::types::TypeKind::Named { path, .. } if path.segments == ["Vec"])
            );
            assert!(
                matches!(&value.as_ref().unwrap().kind, ExprKind::MacroCall { path, delimiter: MacroDelimiter::Bracket, .. } if path.segments == ["vec"])
            );
        }
        _ => panic!("expected let"),
    }
    // stmt 3: for n: i32 in numbers
    match &f.body.stmts[3].kind {
        StmtKind::For { pattern, ty, .. } => {
            assert!(
                matches!(&pattern.kind, nanachi_ast::pattern::PatternKind::Ident(n) if n == "n")
            );
            assert!(ty.is_some());
        }
        _ => panic!("expected for"),
    }
    // stmt 6: while count < 10
    match &f.body.stmts[6].kind {
        StmtKind::While { condition, .. } => {
            assert!(matches!(
                &condition.kind,
                ExprKind::BinaryOp { op: BinOp::Lt, .. }
            ));
        }
        _ => panic!("expected while"),
    }
}

#[test]
fn sketch_async() {
    let prog = parse_str(
        r#"
        async fn fetch(url: String) -> String {
            let response: String = reqwest::get(url).await.text().await;
            response
        }

        async fn main() {
            let data: String = fetch("https://example.com").await;
            println!("{}", data);
        }
    "#,
    );
    assert_eq!(prog.items.len(), 2);
    // async fn fetch
    let fetch = expect_fn(&prog, 0, "fetch");
    assert!(fetch.is_async);
    assert_eq!(fetch.params.len(), 1);
    // body: let response = reqwest::get(url).await.text().await;
    match &fetch.body.stmts[0].kind {
        StmtKind::Let { value, .. } => {
            // .await is the outermost node
            assert!(matches!(
                &value.as_ref().unwrap().kind,
                ExprKind::Await { .. }
            ));
        }
        _ => panic!("expected let"),
    }
    assert!(fetch.body.tail_expr.is_some()); // response
    // async fn main
    let main = expect_fn(&prog, 1, "main");
    assert!(main.is_async);
    // let data = fetch(...).await;
    match &main.body.stmts[0].kind {
        StmtKind::Let { value, .. } => {
            assert!(matches!(
                &value.as_ref().unwrap().kind,
                ExprKind::Await { .. }
            ));
        }
        _ => panic!("expected let"),
    }
}

#[test]
fn sketch_rust_block() {
    let prog = parse_str(
        r#"
        fn interop_example() {
            let x: i32 = 42;
            rust {
                use std::collections::HashMap;
                let mut map = HashMap::new();
                map.insert("answer", x);
                println!("{:?}", map);
            }
        }
    "#,
    );
    let f = expect_fn(&prog, 0, "interop_example");
    assert_eq!(f.body.stmts.len(), 2);
    // stmt 0: let x: i32 = 42;
    assert!(matches!(&f.body.stmts[0].kind, StmtKind::Let { .. }));
    // stmt 1: rust { ... } (should be an Item stmt containing RustBlock)
    match &f.body.stmts[1].kind {
        StmtKind::Item(item) => match &item.kind {
            ItemKind::RustBlock(rb) => {
                assert!(rb.code.contains("HashMap"));
                assert!(rb.code.contains("map"));
            }
            _ => panic!("expected rust block item"),
        },
        _ => panic!("expected item stmt"),
    }
}

#[test]
fn sketch_comprehensive() {
    let prog = parse_str(
        r#"
        fn load_users(path: String) -> Vec<User> {
            let content: String = fs::read_to_string(path);
            let lines: Vec<String> = content.lines().collect();
            let users: Vec<User> = Vec::new();
            for line: String in lines {
                let parts: Vec<String> = line.split(',').collect();
                let name: String = parts[0];
                let age: i32 = parts[1].parse();
                users.push(User::new(name, age));
            }
            users
        }
    "#,
    );
    let f = expect_fn(&prog, 0, "load_users");
    assert_eq!(f.params.len(), 1);
    // return type: Vec<User>
    match &f.return_ty.as_ref().unwrap().kind {
        nanachi_ast::types::TypeKind::Named { path, generics } => {
            assert_eq!(path.segments, vec!["Vec"]);
            assert_eq!(generics.len(), 1);
            assert!(
                matches!(&generics[0].kind, nanachi_ast::types::TypeKind::Named { path, .. } if path.segments == ["User"])
            );
        }
        _ => panic!("expected Vec<User>"),
    }
    // body: 3 let stmts + for + tail expr
    assert_eq!(f.body.stmts.len(), 4); // let content, let lines, let users, for
    assert!(f.body.tail_expr.is_some()); // users
    // stmt 3: for loop with 4 stmts inside
    match &f.body.stmts[3].kind {
        StmtKind::For {
            pattern, ty, body, ..
        } => {
            assert!(
                matches!(&pattern.kind, nanachi_ast::pattern::PatternKind::Ident(n) if n == "line")
            );
            assert!(ty.is_some());
            assert_eq!(body.stmts.len(), 4); // let parts, let name, let age, users.push(...)
        }
        _ => panic!("expected for"),
    }
}

/// Helper: extract FunctionItem from prog.items[idx], assert name matches.
fn expect_fn<'a>(prog: &'a Program, idx: usize, name: &str) -> &'a FunctionItem {
    match &prog.items[idx].kind {
        ItemKind::Function(f) => {
            assert_eq!(f.name, name);
            f
        }
        _ => panic!("expected function '{}' at index {}", name, idx),
    }
}
