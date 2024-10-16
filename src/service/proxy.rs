use super::traffic_filter::TrafficFilter;
use crate::utils::logger::{LogLevel, Logger};
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::{
    body::Bytes, http, server::conn::http1, service::service_fn, upgrade::Upgraded, Method,
    Request, Response, Uri,
};
use hyper_util::rt::TokioIo;
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tokio::net::{TcpListener, TcpStream};

/// The enum that controls the current Proxy status, e.g. Running, Stopped.
#[derive(Debug, PartialEq, Clone, Default)]
pub enum ProxyEvent {
    Starting,
    Running,
    #[default]
    Stopped,
    Error(String),
    Terminating,
    Terminated,
    RequestEvent(ProxyRequestLog),
}

impl ToString for ProxyEvent {
    /// Converts the ProxyEvent to a readable string.
    fn to_string(&self) -> String {
        let current_proxy_status = match self {
            ProxyEvent::Starting => String::from("STARTING"),
            ProxyEvent::Running => String::from("RUNNING"),
            ProxyEvent::Stopped => String::from("STOPPED"),
            ProxyEvent::Error(_) => String::from("ERROR"),
            ProxyEvent::Terminating => String::from("TERMINATING"),
            ProxyEvent::Terminated => String::from("TERMINATED"),
            _ => String::from("UNKNOWN"),
        };

        current_proxy_status
    }
}

/// Contains the information regarding the selected exclusion list value.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
pub struct ProxyExclusionRow {
    pub updating: bool,
    pub index: usize,
    pub value: String,
}

impl Default for ProxyExclusionRow {
    fn default() -> Self {
        Self {
            updating: bool::default(),
            index: usize::default(),
            value: String::default(),
        }
    }
}

/// The enum that controls the action being taken when updating an exclusion list value.
pub enum ProxyExclusionUpdateKind {
    Edit,
    Add,
    Remove,
}

/// Contains the information regarding a request made through the Proxy.
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub struct ProxyRequestLog {
    pub method: String,
    pub request: String,
    pub blocked: bool,
}

impl ProxyRequestLog {
    /// Converts the exclusion status of a request to readable string.
    fn to_blocked_string(&self) -> String {
        match self.blocked {
            true => String::from("BLOCKED"),
            false => String::from("ALLOWED"),
        }
    }
}

/// The enum that carries which UI Window is currently active.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq, Default)]
pub enum ProxyView {
    #[default]
    Min,
    Logs,
    Filter,
}

impl ToString for ProxyView {
    /// Convert the ProxyView enum to a readable string.
    fn to_string(&self) -> String {
        match self {
            ProxyView::Min => String::from("Default View"),
            ProxyView::Logs => String::from("Log View"),
            ProxyView::Filter => String::from("Filter View"),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
#[serde(default)]
pub struct Proxy {
    // Startup related items
    pub port: String,
    pub port_error: String,
    pub start_enabled: bool,

    // Which view is currently showing, one of ProxyView
    pub view: ProxyView,

    // Logger
    pub logger: Logger,

    // The current Proxy status, one of ProxyEvent
    #[serde(skip)]
    pub status: Arc<Mutex<ProxyEvent>>,

    // The current event sender, send one of ProxyEvent
    #[serde(skip)]
    pub event: Arc<Mutex<Option<std::sync::mpsc::Sender<ProxyEvent>>>>,

    // The list of requests to show in the logs
    #[serde(skip)]
    pub requests: Arc<Mutex<Vec<ProxyRequestLog>>>,

    // Traffic Filters
    pub traffic_filter: Arc<Mutex<TrafficFilter>>,

    // Different value selectors for exclusion management
    pub selected_value: String,
    pub selected_exclusion_row: ProxyExclusionRow,

    // Store the current running time of the Proxy
    #[serde(skip)]
    pub run_time: Arc<Mutex<Option<std::time::Instant>>>,
}

/// Creates a new Proxy from default values.
impl Default for Proxy {
    fn default() -> Self {
        let logger = Logger::default();
        let status = Arc::new(Mutex::new(ProxyEvent::default()));
        let requests = Arc::new(Mutex::new(Vec::<ProxyRequestLog>::new()));
        let traffic_filter = Arc::new(Mutex::new(TrafficFilter::default()));
        let run_time = Arc::new(Mutex::new(None));

        Self {
            port: String::default(),
            port_error: String::default(),
            start_enabled: true,
            event: Arc::new(Mutex::new(None)),
            selected_value: String::default(),
            selected_exclusion_row: ProxyExclusionRow::default(),
            status,
            view: ProxyView::default(),
            logger,
            requests,
            traffic_filter,
            run_time,
        }
    }
}

impl Proxy {
    /// Creates a new Proxy from given values.
    ///
    /// # Arguments
    /// * `port` - A String that contains the port.
    /// * `view` - A ProxyView value indicating which view is active.
    /// * `traffic_filter` - A TrafficFilter containing the applied filters.
    /// * `log_level` - The logging level.
    pub fn new(
        port: String,
        view: ProxyView,
        traffic_filter: TrafficFilter,
        log_level: LogLevel,
    ) -> Self {
        let mut logger = Logger::default();
        logger.set_level(log_level);

        let status = Arc::new(Mutex::new(ProxyEvent::default()));
        let requests = Arc::new(Mutex::new(Vec::<ProxyRequestLog>::new()));
        let traffic_filter = Arc::new(Mutex::new(traffic_filter));
        let run_time = Arc::new(Mutex::new(None));

        Self {
            port,
            port_error: String::default(),
            start_enabled: true,
            event: Arc::new(Mutex::new(None)),
            selected_value: String::default(),
            selected_exclusion_row: ProxyExclusionRow::default(),
            status,
            view,
            logger,
            requests,
            traffic_filter,
            run_time,
        }
    }

    /// Begin the proxy service event handler and server.
    pub fn run(&mut self) {
        // Begin handling events
        self.handle_events();

        // Send the starting event
        self.logger.info("Service is now starting...");
        self.send(ProxyEvent::Starting);

        // Start the server
        self.handle_server();
    }

    // Send the stop event for the service.
    pub fn stop(&self) {
        self.logger.info("Service is now stopping...");
        self.send(ProxyEvent::Terminating);
    }

    /// Handles ProxyEvents.
    fn handle_events(&mut self) {
        let (event_sender, event_receiver) = std::sync::mpsc::channel::<ProxyEvent>();

        *self.event.lock().unwrap() = Some(event_sender);

        let run_time = self.run_time.clone();
        let status = self.status.clone();
        let requests = self.requests.clone();
        let event_clone = self.event.clone();
        let logger = self.logger.clone();

        thread::spawn(move || {
            loop {
                // Sleep loop to loosen CPU stress
                thread::sleep(Duration::from_millis(100));

                // Check incoming Proxy events
                match event_receiver.recv() {
                    Ok(event) => match event {
                        // Generic Events
                        ProxyEvent::Starting => {
                            *status.lock().unwrap() = event;
                        }
                        ProxyEvent::Running => {
                            // Start the timer
                            *run_time.lock().unwrap() = Some(std::time::Instant::now());

                            *status.lock().unwrap() = event;
                        }
                        ProxyEvent::Terminated => {
                            logger.global("Service has been stopped.");

                            *status.lock().unwrap() = ProxyEvent::Stopped;

                            // Clear the timer
                            *run_time.lock().unwrap() = None;

                            // Terminate the event_handler, remove the event sync
                            *event_clone.lock().unwrap() = None;

                            break;
                        }
                        ProxyEvent::Error(message) => {
                            *status.lock().unwrap() = ProxyEvent::Error(message);
                        }
                        ProxyEvent::RequestEvent(request_log) => {
                            // We need to have a --no-gui option to enable this
                            // println!(
                            //     "{} {} {}",
                            //     "REQUEST:".green(),
                            //     uri,
                            //     if blocked {
                            //         "-> BLOCKED".red()
                            //     } else {
                            //         "-> ALLOWED".green()
                            //     }
                            // );

                            requests.lock().unwrap().push(request_log.clone());
                        }
                        _ => {
                            *status.lock().unwrap() = event;
                        }
                    },
                    Err(message) => {
                        *status.lock().unwrap() = ProxyEvent::Error(message.to_string())
                    }
                }
            }
        });
    }

    /// Handles the server and server requests.
    fn handle_server(&self) {
        let event = self.event.lock().unwrap().clone();
        let port = self.port.clone();
        let status = Arc::clone(&self.status);
        let traffic_filter = Arc::clone(&self.traffic_filter);
        let logger = self.logger.clone();

        thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    // Termination Signal
                    let mut signal = std::pin::pin!(handle_termination(event.clone(), status));

                    // Bind to address with supplied port
                    let address =
                        SocketAddr::from(([127, 0, 0, 1], port.trim().parse::<u16>().unwrap()));
                    let listener = TcpListener::bind(address).await;

                    // Create a request service
                    let proxy_service_event = event.clone();
                    let request_logger = logger.clone();
                    let proxy_service = service_fn(move |request| {
                        handle_request(
                            request,
                            proxy_service_event.clone(),
                            traffic_filter.lock().unwrap().clone(),
                            request_logger.clone(),
                        )
                    });

                    // Handle service listener events
                    match listener {
                        Ok(listener) => {
                            if let Some(sender) = event.clone() {
                                sender.send(ProxyEvent::Running).unwrap();
                            }

                            logger.global("Service is now running...");

                            loop {
                                tokio::select! {
                                    Ok((stream, _addr)) = listener.accept() => {
                                        let io = TokioIo::new(stream);
                                        let connection = http1::Builder::new()
                                            .preserve_header_case(true)
                                            .title_case_headers(true)
                                            .serve_connection(io, proxy_service.clone())
                                            .with_upgrades();

                                        tokio::task::spawn(async move {
                                            let _ = connection.await;
                                        });
                                    },

                                    _ = &mut signal => break
                                }
                            }
                        }
                        Err(message) => {
                            if let Some(sender) = event.clone() {
                                sender.send(ProxyEvent::Error(message.to_string())).unwrap();
                            }
                        }
                    };
                });
        });
    }

    /// Returns the Proxy's current status.
    pub fn get_status(&mut self) -> ProxyEvent {
        self.status.lock().unwrap().clone()
    }

    // Returns the current logger.
    pub fn get_logger(&self) -> Logger {
        self.logger.clone()
    }

    /// Returns the Proxy's current TrafficFilter.
    pub fn get_traffic_filter(&self) -> TrafficFilter {
        self.traffic_filter.lock().unwrap().clone()
    }

    /// Returns the Proxy's recent requests.
    pub fn get_requests(&self) -> Vec<ProxyRequestLog> {
        self.requests.lock().unwrap().to_vec()
    }

    /// Returns the Proxy's current running time.
    pub fn get_run_time(&mut self) -> String {
        let run_time = self.run_time.lock().unwrap();
        match *run_time {
            Some(duration) => duration.elapsed().as_secs().to_string(),
            None => 0.to_string(),
        }
    }

    /// Send a ProxyEvent.
    ///
    /// # Arguments:
    /// * `event` - A ProxyEvent to send to the event handler.
    pub fn send(&self, event: ProxyEvent) {
        if let Some(sender) = self.event.lock().unwrap().clone() {
            sender.send(event).unwrap();
        }
    }

    /// Toggle the traffic filtering on/off.
    pub fn toggle_traffic_filtering(&self) {
        let mut traffic_filter = self.traffic_filter.lock().unwrap();
        let enabled = traffic_filter.get_enabled();
        traffic_filter.set_enabled(!enabled);
        self.logger.debug("Traffic filtering has been toggled.");
    }

    /// Toggle the traffic filter between: TrafficFilterType::Allow / TrafficFilterType::Deny.
    pub fn switch_exclusion_list(&self) {
        let mut traffic_filter = self.traffic_filter.lock().unwrap();
        let switched_filter = traffic_filter.get_opposing_filter_type();
        traffic_filter.set_filter_type(switched_filter);
        self.logger.debug("Exclusion list has been switched.");
    }

    /// Set the current exclusion list.
    ///
    /// # Arguments:
    /// * `list` - A Vec<String> to set the exclusion list to.
    pub fn set_exclusion_list(&mut self, list: Vec<String>) {
        let mut traffic_filter = self.traffic_filter.lock().unwrap();
        traffic_filter.set_filter_list(list);
        self.logger.debug("Exclusion list has been set.");
    }

    /// Send an event to add a value to the current exclusion list
    ///
    /// # Arguments:
    /// * `event_type` - A ProxyExclusionUpdateKind to Edit/Add/Remove an item in the current exclusion list.
    pub fn update_exclusion_list(&mut self, event_type: ProxyExclusionUpdateKind) {
        match event_type {
            ProxyExclusionUpdateKind::Edit => {
                let mut traffic_filter = self.traffic_filter.lock().unwrap();
                traffic_filter.update_filter_list_item(
                    self.selected_exclusion_row.index,
                    self.selected_exclusion_row.value.clone(),
                );

                self.selected_exclusion_row = ProxyExclusionRow::default();
                self.logger.debug("Exclusion list value has been edited.");
            }
            ProxyExclusionUpdateKind::Add | ProxyExclusionUpdateKind::Remove => {
                let mut traffic_filter = self.traffic_filter.lock().unwrap();
                traffic_filter.update_filter_list(self.selected_value.clone());
                self.logger.debug("Exclusion list has been updated.");
            }
        };
    }
}

/// Handles termination of the service.
///
/// # Arguments
/// * `event` - The event sender to write current state.
/// * `status` - The current ProxyEvent status.
async fn handle_termination(
    event: Option<std::sync::mpsc::Sender<ProxyEvent>>,
    status: Arc<Mutex<ProxyEvent>>,
) {
    let (shutdown_sig, shutdown_rec) = tokio::sync::oneshot::channel::<()>();

    std::thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(1000));

        let status = match status.lock() {
            Ok(status) => status.clone(),
            Err(_) => ProxyEvent::Terminating,
        };

        if status == ProxyEvent::Terminating {
            shutdown_sig.send(()).unwrap();
            break;
        }
    });

    if let Ok(_) = shutdown_rec.await {
        if let Some(event) = event {
            event.send(ProxyEvent::Terminated).unwrap();
        }
    }
}

/// Handle a server request.
///
/// # Arguments:
/// * `request` - The request to proxy.
/// * `event` - An internal event sender, to change the Proxy state.
/// * `traffic_filter` - The current TrafficFilter configuration.
/// * `logger` - The current logger to log events to.
async fn handle_request(
    request: Request<hyper::body::Incoming>,
    event: Option<std::sync::mpsc::Sender<ProxyEvent>>,
    traffic_filter: TrafficFilter,
    logger: Logger,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let request_uri = request.uri().to_string();

    let is_excluded_address = traffic_filter.in_filter_list(&request_uri);
    let is_traffic_blocking = traffic_filter.is_blocking();

    if traffic_filter.get_enabled() {
        let is_blocking_but_exluded = !is_excluded_address && is_traffic_blocking;
        let is_allowing_but_excluded = is_excluded_address && !is_traffic_blocking;
        let blocked = is_allowing_but_excluded || is_blocking_but_exluded;

        // Log the event
        let request_log = ProxyRequestLog {
            method: request.method().to_string(),
            request: request_uri,
            blocked: blocked,
        };

        logger.debug(
            format!(
                "{} -> Request to: {} -> {}",
                request_log.method,
                request_log.request,
                request_log.to_blocked_string()
            )
            .as_str(),
        );

        // Record the request
        if let Some(event) = event {
            event
                .send(ProxyEvent::RequestEvent(request_log.clone()))
                .unwrap();
        }

        // Return a generic response if the request was blocked
        if blocked {
            let mut resp = Response::new(full("Oopsie Whoopsie!"));
            *resp.status_mut() = http::StatusCode::FORBIDDEN;
            return Ok(resp);
        }
    }

    // Proxy socket requests
    if request.method() == Method::CONNECT {
        if let Some(addr) = get_host_address(request.uri()) {
            tokio::task::spawn(async move {
                match hyper::upgrade::on(request).await {
                    Ok(upgraded) => {
                        if let Err(message) = tunnel(upgraded, addr).await {
                            logger.warning(&message.to_string());
                        };
                    }
                    Err(message) => {
                        logger.warning(&message.to_string());
                    }
                }
            });

            return Ok(Response::new(empty()));
        } else {
            let mut resp = Response::new(full("CONNECT must be to a socket address"));
            *resp.status_mut() = http::StatusCode::BAD_REQUEST;
            return Ok(resp);
        }
    }

    // Proxy web requests
    if let Some(host) = request.uri().host() {
        let port = request.uri().port_u16().unwrap_or(80);

        let stream = TcpStream::connect((host, port)).await.unwrap();
        let io = TokioIo::new(stream);

        let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
            .preserve_header_case(true)
            .title_case_headers(true)
            .handshake(io)
            .await?;

        tokio::task::spawn(async move {
            if let Err(message) = conn.await {
                logger.warning(&message.to_string());
            };
        });

        let response = sender.send_request(request).await?;
        return Ok(response.map(|b| b.boxed()));
    } else {
        logger.debug(
            format!(
                "Host address could not be found for: {}",
                request.uri().to_string()
            )
            .as_str(),
        );
        let mut response = Response::new(full("Host address could not be processed."));
        *response.status_mut() = http::StatusCode::BAD_REQUEST;
        return Ok(response);
    }
}

/// Tunnel a connection bidirectionally.
///
/// # Arguments:
/// * `upgraded` - The upgraded connection to copy data to/from.
/// * `address` - The target address to copy data to/from.
async fn tunnel(upgraded: Upgraded, address: String) -> std::io::Result<()> {
    let mut server = TcpStream::connect(address).await?;
    let mut upgraded_connection = TokioIo::new(upgraded);

    tokio::io::copy_bidirectional(&mut upgraded_connection, &mut server).await?;

    Ok(())
}

/// Get the current URI's host address.
///
/// # Arguments
/// * `uri` - The given URI.
fn get_host_address(uri: &Uri) -> Option<String> {
    uri.authority().and_then(|auth| Some(auth.to_string()))
}

/// Create an empty response body.
fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}

/// Create an body from the given bytes.
fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}
