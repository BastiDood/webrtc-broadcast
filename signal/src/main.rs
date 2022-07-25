fn main() -> anyhow::Result<()> {
    use core::convert::Infallible;
    use std::net;

    let tcp = net::TcpListener::bind((net::Ipv4Addr::UNSPECIFIED, 3000))?;
    tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()?
        .block_on(async {
            use core::future;
            use hyper::service;

            let service = service::make_service_fn(|_| {
                future::ready(Ok::<_, Infallible>(service::service_fn(|_| {
                    future::ready(Ok::<_, Infallible>(hyper::Response::new(
                        hyper::Body::empty(),
                    )))
                })))
            });

            hyper::Server::from_tcp(tcp)?
                .http1_only(true)
                .serve(service)
                .await
        })?;
    Ok(())
}
