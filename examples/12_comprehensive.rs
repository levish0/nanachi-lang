fn load_users(path: &str) -> Result<Vec<User>, std::io::Error> {
    let content: String = fs::read_to_string(path)?;
    let lines: Vec<String> = content.lines().collect();
    let users: Vec<User> = Vec::new();
    for line in lines {
        let parts: Vec<String> = line.split(&(',')).collect();
        let name: String = (parts)[0];
        let age: i32 = (parts)[1].parse();
        users.push(User::new(&(name), &(age)));
    }
    Ok(users)
}
