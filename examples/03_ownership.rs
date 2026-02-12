fn greet(name: &str) {
    println!("Hello, {}" , name);
}

fn push_name(list: &mut Vec<String>, name: String) {
    list.push(name);
}

fn ownership_example() {
    let a: String = String::from("hello");
    let b: String = String::from("world");
    let list: Vec<String> = Vec::new();
    greet(&(a));
    greet(&(b));
    push_name(&mut (list), a);
    push_name(&mut (list), (b).clone());
    println!("{}" , b);
}
