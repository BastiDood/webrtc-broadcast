mod state;
mod ws;

fn main() -> anyhow::Result<()> {
    use core::convert::Infallible;
    use std::{net, sync::Arc};

    let tcp = net::TcpListener::bind((net::Ipv4Addr::UNSPECIFIED, 3000))?;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_io().enable_time().build()?;
    let state = Arc::new(state::State::try_new()?);

    let future = {
        use hyper::{service, Body, Response};
        let _guard = runtime.enter();
        let service = service::make_service_fn(|_| {
            use core::future;
            let state_outer = state.clone();
            future::ready(Ok::<_, Infallible>(service::service_fn(move |req| {
                let response = state_outer
                    .clone()
                    .on_request(req)
                    .unwrap_or_else(|code| {
                        let mut response = Response::new(Body::empty());
                        *response.status_mut() = code;
                        response
                    });
                future::ready(Ok::<_, Infallible>(response))
            })))
        });
        hyper::Server::from_tcp(tcp)?.http1_only(true).serve(service)
    };

    runtime.block_on(future)?;
    Ok(())
}
