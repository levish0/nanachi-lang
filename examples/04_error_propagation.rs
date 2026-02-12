fn read_config(path: &str) -> Result<String, std::io::Error> {
    let content: String = fs::read_to_string(path)?;
    Ok(content)
}

fn process() -> Result<(), std::io::Error> {
    let config: String = read_config(&(String::from("config.toml")))?;
    println!("{}", config);
    Ok(())
}
