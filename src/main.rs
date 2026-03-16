use tracing::info;

pub mod agent;
pub mod llm;
pub mod tool;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    // build ds
    let user = llm::deep_seek::Message::new(
        llm::deep_seek::Role::User,
        "给我获取当前系统时间".to_string(),
    );
    let mut messages = vec![user];

    let mut react = agent::ReAct::new(messages.clone());
    let answer = react.execute().await;
    messages.push(answer.unwrap());

    info!("{:?}", messages);
}
