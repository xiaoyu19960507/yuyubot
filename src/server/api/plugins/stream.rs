use crate::plus::PluginManager;
use rocket::{
    get,
    response::stream::{Event, EventStream},
    State,
};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

#[derive(Serialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum PluginUnifiedEvent {
    Output(crate::plus::manager::PluginOutputEvent),
    Status(crate::plus::manager::PluginStatusEvent),
}

#[get("/plugins/events_stream")]
pub fn plugins_events_stream(manager: &State<Arc<PluginManager>>) -> EventStream![Event + 'static] {
    let manager = manager.inner().clone();
    EventStream! {
        let mut rx_output = manager.subscribe_output();
        let mut rx_status = manager.subscribe_status();

        loop {
            let event = tokio::select! {
                res = rx_output.recv() => match res {
                    Ok(e) => Some(PluginUnifiedEvent::Output(e)),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                    Err(_) => None,
                },
                res = rx_status.recv() => match res {
                    Ok(e) => Some(PluginUnifiedEvent::Status(e)),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                    Err(_) => None,
                },
            };

            if let Some(event) = event {
                if let Ok(json) = serde_json::to_string(&event) {
                    yield Event::data(json);
                }
            } else {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
}

#[get("/plugins/status_stream")]
pub fn plugins_status_stream(manager: &State<Arc<PluginManager>>) -> EventStream![Event + 'static] {
    let manager = manager.inner().clone();
    EventStream! {
        let mut rx = manager.subscribe_status();

        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Ok(json) = serde_json::to_string(&event) {
                        yield Event::data(json);
                    }
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }
}

#[get("/plugins/<plugin_id>/output/stream")]
pub fn plugin_output_stream(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> EventStream![Event + 'static] {
    let manager = manager.inner().clone();
    let target_plugin = plugin_id.clone();
    EventStream! {
        if let Ok(output) = manager.get_plugin_output(&plugin_id).await {
            for line in &output {
                if let Ok(json) = serde_json::to_string(&line) {
                    yield Event::data(json);
                }
            }
        }

        let mut rx = manager.subscribe_output();

        loop {
            match rx.recv().await {
                Ok(event) => {
                    if event.plugin_id == target_plugin {
                        if let Ok(json) = serde_json::to_string(&event.line) {
                            yield Event::data(json);
                        }
                    }
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }
}
