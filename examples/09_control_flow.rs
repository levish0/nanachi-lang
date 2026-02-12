fn control_flow() {
    let x: i32 = 10;
    let abs: i32 = if (x > 0) { x } else { (-x) };
    let numbers: Vec<i32> = vec![1, 2, 3, 4, 5];
    for n in &(numbers) {
        println!("{}", n);
    }
    println!("len: {}", numbers.len());
    let mut count: i32 = 0;
    while (count < 10) {
        count = (count + 1);
    }
}
