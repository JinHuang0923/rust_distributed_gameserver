use tracing::info;

mod config;
mod distribute;
mod heartbeat;
mod main_handle;
mod server_context;
mod server_run;
mod world;
mod http_rpc;
// mod snow_id;

#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(tracing::Level::DEBUG)
        // completes the builder.
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    let args: Vec<String> = std::env::args().collect();
    dbg!(args.clone());
    let (_, world_update_handle) = server_run::run(args[1].clone()).await;
    info!("Hello, server!");

    world_update_handle.await.unwrap();
}
#[cfg(test)]
mod main_tests {
    use serde_json::Value;

    
    #[tokio::test]
    async fn get_master() {
        let body = reqwest::get("http://127.0.0.1:3090/master")
            .await.unwrap()
            .text()
            .await.unwrap();
        //tojson
        let result_json = serde_json::from_str::<Value>(&body).unwrap();
        // master_addr["master_addr"].as_str().unwrap();
        println!("body {:?}",result_json);

    }
}
