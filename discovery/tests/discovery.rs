use exocore_discovery::{client::Error, Client, Server};

#[tokio::test(threaded_scheduler)]
async fn golden_path() -> anyhow::Result<()> {
    let server = Server::new(3010);
    tokio::task::spawn(async move {
        server.start().await.unwrap();
    });

    tokio::time::delay_for(std::time::Duration::from_millis(100)).await;

    let client = Client::new("http://127.0.0.1:3010")?;
    let create_resp = client.create(b"hello world").await?;

    let get_res = client.get(1337).await;
    match get_res {
        Err(Error::NotFound) => {}
        _other => panic!("Expected not found error"),
    }

    let get_res = client.get(create_resp.id).await?;
    assert_eq!(b"hello world", get_res.as_slice());

    Ok(())
}
