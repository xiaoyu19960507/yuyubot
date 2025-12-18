use crate::plus::PluginManager;
use crate::server::api::BotConfig;
use futures_util::StreamExt;
use rocket::data::{Data, ToByteUnit};
use rocket::fairing::AdHoc;
use rocket::http::{ContentType, Status};
use rocket::request::{FromRequest, Outcome};
use rocket::response::stream::{Event, EventStream};
use rocket::response::{Responder, Response};
use rocket::Request;
use rocket::{get, post, routes, Config, State};
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use ws as rocket_ws;

#[derive(Clone)]
struct SseMessage {
    event: Option<String>,
    data: String,
}

struct ForwardHeaders {
    content_type: Option<String>,
    accept: Option<String>,
}

struct PluginAuth {
    plugin_id: String,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ForwardHeaders {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        Outcome::Success(ForwardHeaders {
            content_type: req.content_type().map(|ct| ct.to_string()),
            accept: req.headers().get_one("Accept").map(|v| v.to_string()),
        })
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for PluginAuth {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let manager = match req.rocket().state::<Arc<PluginManager>>() {
            Some(m) => m,
            None => return Outcome::Error((Status::InternalServerError, ())),
        };

        let token = extract_access_token(req);
        if token.is_empty() {
            return Outcome::Error((Status::Unauthorized, ()));
        }

        match manager.get_plugin_id_by_api_token(&token).await {
            Some(plugin_id) => Outcome::Success(PluginAuth { plugin_id }),
            None => Outcome::Error((Status::Unauthorized, ())),
        }
    }
}

pub struct MilkyApiProxy {
    bot_config: Arc<RwLock<BotConfig>>,
    client: reqwest::Client,
}

pub struct MilkyEventProxy {
    bot_config: Arc<RwLock<BotConfig>>,
    client: reqwest::Client,
    tx: broadcast::Sender<SseMessage>,
    clients: AtomicUsize,
    ws_tx: broadcast::Sender<String>,
    ws_clients: AtomicUsize,
}

use rocket::{Ignite, Rocket};
use tokio::task::JoinHandle;

pub async fn spawn_milky_proxy_servers(
    api_port: u16,
    event_port: u16,
    bot_config: Arc<RwLock<BotConfig>>,
    plugin_manager: Arc<PluginManager>,
) -> Result<
    (
        JoinHandle<Result<Rocket<Ignite>, rocket::Error>>,
        JoinHandle<Result<Rocket<Ignite>, rocket::Error>>,
    ),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let api_proxy = Arc::new(MilkyApiProxy {
        bot_config: bot_config.clone(),
        client: reqwest::Client::new(),
    });

    let (tx, _) = broadcast::channel::<SseMessage>(2048);
    let (ws_tx, _) = broadcast::channel::<String>(2048);
    let event_proxy = Arc::new(MilkyEventProxy {
        bot_config: bot_config.clone(),
        client: reqwest::Client::new(),
        tx,
        clients: AtomicUsize::new(0),
        ws_tx,
        ws_clients: AtomicUsize::new(0),
    });

    tokio::spawn(run_upstream_sse(event_proxy.clone()));

    // Prepare API Rocket
    let address: std::net::IpAddr = "127.0.0.1".parse().unwrap();
    let api_config = Config {
        address,
        port: api_port,
        log_level: rocket::config::LogLevel::Off,
        ..Config::default()
    };

    let plugin_manager_clone_for_api = plugin_manager.clone();
    let api_rocket = rocket::custom(api_config)
        .manage(api_proxy.clone())
        .mount("/", routes![proxy_api])
        .manage(plugin_manager.clone())
        .attach(AdHoc::on_liftoff("Get API Port", move |rocket| {
            Box::pin(async move {
                let port = rocket.config().port;
                log_info!("🚀 Milky API 实际上监听的端口是: {}", port);
                plugin_manager_clone_for_api.set_milky_proxy_api_port(port);
            })
        }))
        .ignite()
        .await?;

    // Prepare Event Rocket
    let event_config = Config {
        address,
        port: event_port,
        log_level: rocket::config::LogLevel::Off,
        ..Config::default()
    };

    let plugin_manager_clone_for_event = plugin_manager.clone();
    let event_rocket = rocket::custom(event_config)
        .manage(event_proxy.clone())
        .mount("/", routes![event_ws, event_stream])
        .manage(plugin_manager.clone())
        .attach(AdHoc::on_liftoff("Get Event Port", move |rocket| {
            Box::pin(async move {
                let port = rocket.config().port;
                log_info!("🚀 Milky Event 实际上监听的端口是: {}", port);
                plugin_manager_clone_for_event.set_milky_proxy_event_port(port);
            })
        }))
        .ignite()
        .await?;

    // Launch both
    let api_handle = tokio::spawn(api_rocket.launch());
    let event_handle = tokio::spawn(event_rocket.launch());

    Ok((api_handle, event_handle))
}

pub struct ProxyBytesResponse {
    status: Status,
    content_type: ContentType,
    body: Vec<u8>,
}

impl<'r> Responder<'r, 'static> for ProxyBytesResponse {
    fn respond_to(self, _: &'r Request<'_>) -> rocket::response::Result<'static> {
        Response::build()
            .status(self.status)
            .header(self.content_type)
            .sized_body(self.body.len(), Cursor::new(self.body))
            .ok()
    }
}

#[post("/api/<api>", data = "<data>")]
async fn proxy_api(
    api: &str,
    data: Data<'_>,
    auth: PluginAuth,
    headers: ForwardHeaders,
    proxy: &State<Arc<MilkyApiProxy>>,
) -> Result<ProxyBytesResponse, Status> {
    let body = data
        .open(4.mebibytes())
        .into_bytes()
        .await
        .map_err(|_| Status::BadRequest)?
        .value;

    let config = proxy.bot_config.read().await.clone();
    let url = format!("{}/{}", config.get_api_url(), api);

    let mut builder = proxy.client.post(url);

    if let Some(ct) = headers.content_type {
        builder = builder.header("Content-Type", ct);
    } else {
        builder = builder.header("Content-Type", "application/json");
    }

    if let Some(accept) = headers.accept {
        builder = builder.header("Accept", accept);
    }

    if let Some(token) = config.token {
        builder = builder.header("Authorization", format!("Bearer {}", token));
    }

    builder = builder.header("X-YUYU-PLUGIN-ID", auth.plugin_id);

    let response = builder
        .body(body)
        .send()
        .await
        .map_err(|_| Status::BadGateway)?;

    let status = Status::new(response.status().as_u16());
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .and_then(ContentType::parse_flexible)
        .unwrap_or(ContentType::JSON);

    let bytes = response
        .bytes()
        .await
        .map_err(|_| Status::BadGateway)?
        .to_vec();

    Ok(ProxyBytesResponse {
        status,
        content_type,
        body: bytes,
    })
}

#[get("/event", rank = 1, format = "text/event-stream")]
fn event_stream(
    _auth: PluginAuth,
    proxy: &State<Arc<MilkyEventProxy>>,
) -> EventStream![Event + 'static] {
    let proxy = proxy.inner().clone();

    EventStream! {
        proxy.clients.fetch_add(1, Ordering::SeqCst);
        struct Guard(Arc<MilkyEventProxy>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.clients.fetch_sub(1, Ordering::SeqCst);
            }
        }
        let _guard = Guard(proxy.clone());

        let mut rx = proxy.tx.subscribe();
        while let Ok(msg) = rx.recv().await {
            let mut ev = Event::data(msg.data);
            if let Some(name) = msg.event {
                ev = ev.event(name);
            }
            yield ev;
        }
    }
}

#[get("/event", rank = 2)]
fn event_ws(
    ws: rocket_ws::WebSocket,
    _auth: PluginAuth,
    proxy: &State<Arc<MilkyEventProxy>>,
) -> rocket_ws::Channel<'static> {
    let proxy = proxy.inner().clone();
    proxy.ws_clients.fetch_add(1, Ordering::SeqCst);

    struct Guard(Arc<MilkyEventProxy>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.ws_clients.fetch_sub(1, Ordering::SeqCst);
        }
    }
    let guard = Guard(proxy.clone());

    ws.channel(move |mut stream| {
        Box::pin(async move {
            use rocket::futures::{SinkExt, StreamExt};

            let mut rx = proxy.ws_tx.subscribe();
            let mut inbound_closed = false;

            loop {
                tokio::select! {
                    msg = stream.next(), if !inbound_closed => {
                        match msg {
                            Some(Ok(m)) => {
                                if m.is_close() {
                                    inbound_closed = true;
                                }
                            }
                            Some(Err(_)) | None => {
                                inbound_closed = true;
                            }
                        }
                    }
                    msg = rx.recv() => {
                        match msg {
                            Ok(text) => {
                                if stream.send(rocket_ws::Message::Text(text)).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
            }

            drop(guard);
            Ok(())
        })
    })
}

async fn run_upstream_sse(proxy: Arc<MilkyEventProxy>) {
    loop {
        if proxy.clients.load(Ordering::SeqCst) == 0 && proxy.ws_clients.load(Ordering::SeqCst) == 0
        {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            continue;
        }

        let config = proxy.bot_config.read().await.clone();
        let url = config.get_event_url();

        let mut request_builder = proxy
            .client
            .get(&url)
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache");

        if let Some(token) = config.token {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", token));
        }

        let response = match request_builder.send().await {
            Ok(r) => r,
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        if !response.status().is_success() {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            continue;
        }

        let mut current_event: Option<String> = None;
        let mut data_lines: Vec<String> = Vec::new();
        let mut buffer = String::new();
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            if proxy.clients.load(Ordering::SeqCst) == 0
                && proxy.ws_clients.load(Ordering::SeqCst) == 0
            {
                break;
            }

            let bytes = match chunk {
                Ok(b) => b,
                Err(_) => break,
            };

            let text = match std::str::from_utf8(&bytes) {
                Ok(t) => t,
                Err(_) => continue,
            };

            buffer.push_str(text);

            while let Some(pos) = buffer.find('\n') {
                let mut line = buffer[..pos].to_string();
                buffer.drain(..=pos);

                if line.ends_with('\r') {
                    line.pop();
                }

                if line.is_empty() {
                    if data_lines.is_empty() && current_event.is_none() {
                        continue;
                    }

                    let data = data_lines.join("\n");
                    let _ = proxy.tx.send(SseMessage {
                        event: current_event.clone(),
                        data: data.clone(),
                    });
                    let _ = proxy.ws_tx.send(data);
                    current_event = None;
                    data_lines.clear();
                    continue;
                }

                if let Some(v) = line.strip_prefix("event:") {
                    current_event = Some(v.trim().to_string());
                    continue;
                }

                if let Some(v) = line.strip_prefix("data:") {
                    data_lines.push(v.trim_start().to_string());
                    continue;
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

fn extract_access_token(req: &Request<'_>) -> String {
    if let Some(auth) = req.headers().get_one("Authorization") {
        let token = auth.strip_prefix("Bearer ").unwrap_or(auth).trim();
        if !token.is_empty() {
            return token.to_string();
        }
    }

    let Some(query) = req.uri().query() else {
        return String::new();
    };

    for pair in query.split('&') {
        if let Some(v) = pair.strip_prefix("access_token=") {
            if !v.is_empty() {
                return v.to_string();
            }
        }
    }

    String::new()
}
