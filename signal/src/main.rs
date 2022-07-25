use hyper::{upgrade::OnUpgrade, Body, Request, Response, StatusCode};
use tokio_tungstenite::tungstenite::Message;

async fn handle_ws(up: OnUpgrade, mut callback: impl FnMut(Message)) {
    use futures_util::TryStreamExt;
    let io = up.await.unwrap();
    let mut stream = tokio_tungstenite::accept_async(io).await.unwrap();
    while let Some(msg) = stream.try_next().await.unwrap() {
        callback(msg);
    }
}

fn handle(req: Request<Body>) -> Result<Response<Body>, StatusCode> {
    let path_and_query = req.uri().path_and_query().ok_or(StatusCode::BAD_REQUEST)?;
    let path = path_and_query.path();
    let query = path_and_query.query().unwrap_or_default();

    use hyper::{header, upgrade, Method};
    match *req.method() {
        Method::GET => {
            let callback = match path {
                "/ws/client" => |_| todo!(),
                "/ws/host" => |_| todo!(),
                _ => return Err(StatusCode::NOT_FOUND),
            };

            let accept = signal::ws::validate_headers(req.headers()).ok_or(StatusCode::UPGRADE_REQUIRED)?;
            let value = HeaderValue::from_str(&accept).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let up = upgrade::on(req);
            tokio::spawn(handle_ws(up, callback));

            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::SWITCHING_PROTOCOLS;

            use hyper::http::HeaderValue;
            let headers = response.headers_mut();
            headers.insert(header::CONNECTION, HeaderValue::from_static("Upgrade"));
            headers.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
            headers.insert(header::SEC_WEBSOCKET_ACCEPT, value);
            Ok(response)
        }
        Method::POST => match path {
            "/api/client" => todo!(),
            "/api/host" => todo!(),
            _ => Err(StatusCode::NOT_FOUND),
        },
        _ => Err(StatusCode::METHOD_NOT_ALLOWED),
    }
}

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
                let response = handle(req).unwrap_or_else(|code| {
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
