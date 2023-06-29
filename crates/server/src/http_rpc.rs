use common::{node::NodeInfo, result::Resp};

pub async fn http_get_master_node() -> Option<NodeInfo> {
    let body = reqwest::get("http://127.0.0.1:3090/master")
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    //tojson
    let resp = serde_json::from_str::<Resp<NodeInfo>>(&body).unwrap();
    // master_addr["master_addr"].as_str().unwrap();
    // println!("body {:?}",result_json);
    if resp.code == 500 {
        None
    } else {
        Some(resp.data)
    }
}
