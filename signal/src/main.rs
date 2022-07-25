fn main() -> anyhow::Result<()> {
    use core::convert::Infallible;
    use std::net;

    let tcp = net::TcpListener::bind((net::Ipv4Addr::UNSPECIFIED, 3000))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()?;

    let future = {
        use core::future;
        use hyper::service;

        let _guard = runtime.enter();
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
    };

    runtime.block_on(future)?;
    Ok(())
}
