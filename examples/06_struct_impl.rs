struct User {
    name: String,
    age: i32,
}

impl User {
    fn new(name: String, age: i32) -> User {
        User { name, age }
    }

    fn greet(&self) {
        println!("Hi, I'm {} ({})", self.name, self.age);
    }

    fn grow(&mut self) {
        (self).age = ((self).age + 1);
    }
}
