use common::{
    node::{NodeInfo, NodeType},
    result::Resp,
};

pub async fn get_server_connection() -> Option<NodeInfo> {
    let body = reqwest::get("http://127.0.0.1:3090/connection")
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    //tojson
    let resp = serde_json::from_str::<Resp<NodeInfo>>(&body).unwrap();
    // master_addr["master_addr"].as_str().unwrap();

    if resp.code == 500 {
        None
    } else {
        Some(resp.data)
    }
}

// #[tokio::test]
async fn test_get_server_connection() {
    let node_info = get_server_connection().await;
    println!("node_info {:?}", node_info);
}
