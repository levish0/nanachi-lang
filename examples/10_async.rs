async fn fetch(url: &str) -> String {
    let response: String = ((reqwest::get(url)).await.expect("fallible call failed").text()).await.expect("fallible call failed");
    response
}

#[tokio::main]
async fn main() {
    let data: String = (fetch(&(String::from("https://example.com")))).await;
    println!("{}", data);
}
