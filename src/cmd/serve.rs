use std::{
    convert::Infallible,
    env, fs,
    future::Future,
    io,
    net::SocketAddr,
    path::PathBuf,
    pin::Pin,
    task::{Context, Poll},
};

use anyhow::Result;
use bytes::Bytes;
use fastwebsockets::Frame;
use http_body_util::Full;
use hyper::{body::Incoming, server::conn::http1, Method, Request, Response, StatusCode};
use hyper_util::{rt::TokioIo, service::TowerToHyperService};
use tokio::{
    net::TcpListener,
    sync::broadcast::{self, Sender},
};
use tower::Service;
use tower_http::services::ServeDir;

use super::build::watch_build;
use crate::Generator;

pub(crate) async fn run_serve<G>(
    generator: G,
    source: &str,
    mut port: u16,
    open_browser: bool,
    name: &str,
    bannel: Option<&str>,
) -> Result<()>
where
    G: Generator + Send + 'static,
{
    loop {
        let tmp_dir = env::temp_dir().join(format!("__{}_build", name));
        if tmp_dir.exists() {
            // Remove cached build directory to invalidate the old cache.
            fs::remove_dir_all(&tmp_dir)?;
        }
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let serving_url = format!("http://{addr}");

        match TcpListener::bind(addr).await {
            Ok(listener) => {
                if let Some(bannel) = bannel {
                    println!("{}", bannel);
                }
                println!("listening on {}", serving_url);

                let (tx, mut rx) = broadcast::channel(16);
                let serve_dir =
                    ServeDir::new(&tmp_dir).fallback(FallbackService { tx: tx.clone() });

                if open_browser {
                    tokio::spawn(async move {
                        if rx.recv().await.is_ok() {
                            opener::open(serving_url).unwrap();
                        }
                    });
                }

                let s = PathBuf::from(source);
                tokio::spawn(async move {
                    if let Err(err) = watch_build(generator, s, tmp_dir, true, Some(tx)).await {
                        // handle the error here, for example by logging it or returning it to the caller
                        println!("Watch build error: {err}");
                    }
                });
                let (stream, _) = listener.accept().await?;
                let io = TokioIo::new(stream);

                let svc = TowerToHyperService::new(serve_dir);
                if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                    println!("Error serving connection: {:?}", err);
                }
                break;
            }
            Err(error) => {
                // if the error is address already in use
                // prompt the user to try another port
                if error.kind() == io::ErrorKind::AddrInUse {
                    port = promptly::prompt_default(
                        "Address already in use, try another port?",
                        port + 1,
                    )?;
                    continue;
                }

                println!("Error: {}", error);
                break;
            }
        }
    }
    Ok(())
}

// A fallback service to handle websocket request and ServeDir's 404 request.
#[derive(Clone)]
struct FallbackService {
    tx: Sender<()>,
}

impl Service<Request<Incoming>> for FallbackService {
    type Response = Response<Full<Bytes>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut req: Request<Incoming>) -> Self::Future {
        let mut reload_rx = self.tx.subscribe();
        let fut = async move {
            let path = req.uri().path();
            match (req.method(), path) {
                (&Method::GET, "/live_reload") => {
                    // Check if the request is a websocket upgrade request.
                    if fastwebsockets::upgrade::is_upgrade_request(&req) {
                        let (response, websocket) =
                            fastwebsockets::upgrade::upgrade(&mut req).unwrap();

                        // Spawn a task to handle the websocket connection.
                        tokio::spawn(async move {
                            let mut websocket = websocket.await.unwrap();
                            loop {
                                match reload_rx.recv().await {
                                    Ok(_) => {
                                        if websocket
                                            .write_frame(Frame::text("reload".as_bytes().into()))
                                            .await
                                            .is_err()
                                        {
                                            // Ignore the send failure, the reason could be: Broken pipe
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        panic!("Failed to receive reload signal: {:?}", e);
                                    }
                                }
                            }
                        });

                        // Return the response so the spawned future can continue.
                        Ok(response.map(|_| Full::from("")))
                    } else {
                        Ok(Response::new(Full::from("Not a websocket request!")))
                    }
                }
                _ => {
                    // Return 404 not found response.
                    let resp = Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Full::from("404 Not Found"))
                        .unwrap();
                    Ok(resp)
                }
            }
        };
        Box::pin(fut)
    }
}
