# Examples

## Lex

```sh
cargo run -p nanachi-lexer --example lex_file -- examples/hello.nanachi
```

Output: `examples/hello.tokens`

## Parse

```sh
cargo run -p nanachi-parser --example parse_file -- examples/hello.nanachi
```

Output: `examples/hello.ast`
