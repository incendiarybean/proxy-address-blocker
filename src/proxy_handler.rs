use crate::default_window::ProxyEvent;
use eframe::egui;
use hyper::service::{make_service_fn, service_fn};
use hyper::upgrade::Upgraded;
use hyper::{http, Body, Client, Method, Request, Response, Server};
use serde::de::DeserializeOwned;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::oneshot::{channel, Sender};
type HttpClient = Client<hyper::client::HttpConnector>;

fn handle_termination(
    shutdown_sig: Sender<()>,
    proxy_event_sender: mpsc::Sender<ProxyEvent>,
    status: Arc<Mutex<ProxyEvent>>,
) {
    loop {
        let current_proxy_state = match status.lock() {
            Ok(proxy_event) => proxy_event,
            Err(poisoned) => poisoned.into_inner(),
        };

        match *current_proxy_state {
            ProxyEvent::Terminating => {
                println!("Terminating Service.");
                shutdown_sig.send(()).unwrap();
                break;
            }
            _ => (),
        };
    }

    // Sleep to ensure that termination is complete
    thread::sleep(Duration::from_millis(1000));
    proxy_event_sender.send(ProxyEvent::Terminated).unwrap();
}

#[tokio::main]
pub async fn proxy_service(
    addr: SocketAddr,
    proxy_event_sender: mpsc::Sender<ProxyEvent>,
    status: Arc<Mutex<ProxyEvent>>,
) {
    let addr = addr;

    // Create a oneshot channel for sending a single burst of a termination signal
    let (shutdown_sig, shutdown_rec) = channel::<()>();

    let client = Client::builder()
        .http1_title_case_headers(true)
        .http1_preserve_header_case(true)
        .build_http();

    let make_service = make_service_fn(move |_| {
        let client = client.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let new_proxy = Proxy::new(client.clone(), req);
                new_proxy.proxy()
            }))
        }
    });

    // I try to bind here to check if the Port is available to bind to
    let server = Server::try_bind(&addr);
    match server {
        Ok(builder) => {
            proxy_event_sender.send(ProxyEvent::Running).unwrap();

            // Create handler for monitoring ProxyEvent - Termination Status
            let proxy_event_sender_clone = proxy_event_sender.clone();
            thread::spawn(move || {
                handle_termination(shutdown_sig, proxy_event_sender_clone, status);
            });

            // Create server
            let server = builder
                .http1_preserve_header_case(true)
                .http1_title_case_headers(true)
                .serve(make_service)
                .with_graceful_shutdown(async {
                    shutdown_rec.await.ok();
                });

            // Run server non-stop unless there's an error
            if let Err(_) = server.await {
                proxy_event_sender.send(ProxyEvent::Error).unwrap();
            }
        }
        Err(_) => proxy_event_sender.send(ProxyEvent::Error).unwrap(),
    }
}

struct Proxy {
    client: HttpClient,
    req: Request<Body>,
}

impl Proxy {
    pub fn new(client: HttpClient, req: Request<Body>) -> Self {
        Self { client, req }
    }

    async fn proxy(self) -> Result<Response<Body>, hyper::Error> {
        // Check if address is within blocked list, send FORBIDDEN response on bad request
        let blocked_address = Self::is_blocked_addr(self.req.uri().to_string());
        if blocked_address {
            let mut resp = Response::new(Body::from("Oopsie Whoopsie!"));
            *resp.status_mut() = http::StatusCode::FORBIDDEN;

            return Ok(resp);
        }

        // Forward the rest of accepted requests
        if Method::CONNECT == self.req.method() {
            if let Some(addr) = Self::host_addr(self.req.uri()) {
                tokio::task::spawn(async move {
                    match hyper::upgrade::on(self.req).await {
                        Ok(upgraded) => {
                            if let Err(_) = Self::tunnel(upgraded, addr).await {
                                // self.sender
                                // Self::recent_errors.push(format!("server io error: {}", e));
                            };
                        }
                        Err(e) => println!("upgrade error: {}", e),
                    }
                });

                Ok(Response::new(Body::empty()))
            } else {
                let mut resp = Response::new(Body::from("CONNECT must be to a socket address"));
                *resp.status_mut() = http::StatusCode::BAD_REQUEST;

                Ok(resp)
            }
        } else {
            self.client.request(self.req).await
        }
    }

    pub fn is_blocked_addr(uri: String) -> bool {
        let allowed_uri_list = match read_from_csv::<String>("./src/whitelist.csv") {
            Ok(uri_list) => uri_list,
            Err(_) => Vec::new(),
        };

        let is_blocked = {
            let mut is_blocked = true;

            for item in allowed_uri_list {
                if uri.contains(&item) {
                    is_blocked = false;
                    break;
                }
            }

            is_blocked
        };

        is_blocked
    }

    fn host_addr(uri: &http::Uri) -> Option<String> {
        uri.authority().and_then(|auth| Some(auth.to_string()))
    }

    async fn tunnel(mut upgraded: Upgraded, addr: String) -> std::io::Result<()> {
        let mut server = TcpStream::connect(addr).await?;
        tokio::io::copy_bidirectional(&mut upgraded, &mut server).await?;

        Ok(())
    }
}

fn read_from_csv<CSVRecord>(file_path: &str) -> Result<Vec<CSVRecord>, std::io::Error>
where
    CSVRecord: DeserializeOwned,
{
    let mut rows: Vec<CSVRecord> = Vec::new();
    let mut raw_csv = csv::Reader::from_path(file_path)?;

    for result in raw_csv.deserialize() {
        let row: CSVRecord = result?;
        rows.push(row);
    }

    Ok(rows)
}
