async fn fetch(url: &str) -> String {
    let response: String = ((reqwest::get(&(url))).await.text()).await;
    response
}

#[tokio::main]
async fn main() {
    let data: String = (fetch(&(String::from("https://example.com")))).await;
    println!("{}" , data);
}
