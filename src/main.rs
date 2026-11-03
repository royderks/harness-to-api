#[tokio::main]
async fn main() {
    let code = harness_to_api::cli::run(std::env::args().skip(1).collect()).await;
    std::process::exit(code);
}
