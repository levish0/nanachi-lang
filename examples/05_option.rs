fn find_user(id: &i32) -> Option<User> {
    if (id == 1) { Some(User { name: String::from("nanachi"), age: 3 }) } else { None }
}

fn option_example() {
    let user: Option<User> = find_user(&(42));
    let name: Option<String> = (find_user(&(1))).map(|v| v.name);
    let age: i32 = ((find_user(&(1))).map(|v| v.age)).unwrap_or_else(|| 0);
}
