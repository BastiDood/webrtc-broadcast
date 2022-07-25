fn main() -> anyhow::Result<()> {
    use core::convert::Infallible;
    use std::net;

    let tcp = net::TcpListener::bind((net::Ipv4Addr::UNSPECIFIED, 3000))?;
    let runtime = tokio::runtime::Builder::new_current_thread().enable_io().build()?;

    let future = {
        use hyper::service;
        let _guard = runtime.enter();
        let service = service::make_service_fn(|_| {
            use core::future;
            future::ready(Ok::<_, Infallible>(service::service_fn(|req| {
                use hyper::{http, Method};
                let (http::request::Parts { method, uri, headers, .. }, body) = req.into_parts();
                async move {
                    use hyper::{Body, Response, StatusCode};

                    let http::uri::Parts { ref path_and_query, .. } = uri.into_parts();
                    let (path, query) = path_and_query
                        .as_ref()
                        .map(|pq| (pq.path(), pq.query().unwrap_or_default()))
                        .unwrap_or_default();

                    let (code, body) = match method {
                        Method::GET => match path {
                            "/ws/client" => todo!(),
                            "/ws/host" => todo!(),
                            _ => (StatusCode::NOT_FOUND, Body::empty()),
                        },
                        Method::POST => match path {
                            "/api/client" => todo!(),
                            "/api/host" => todo!(),
                            _ => (StatusCode::NOT_FOUND, Body::empty()),
                        },
                        _ => (StatusCode::METHOD_NOT_ALLOWED, Body::empty()),
                    };

                    let mut response = Response::new(body);
                    *response.status_mut() = code;
                    Ok::<_, Infallible>(response)
                }
            })))
        });

        hyper::Server::from_tcp(tcp)?.http1_only(true).serve(service)
    };

    runtime.block_on(future)?;
    Ok(())
}
