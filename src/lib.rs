use std::collections::HashMap;
use std::sync::mpsc::{Receiver, sync_channel, SyncSender};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use chrono::Local;
use tonic::Request;
use tonic::transport::Channel;
use tracing::{Dispatch, Event, Id, Level, Metadata, span, Subscriber};
use tracing::field::Field;
use tracing::level_filters::LevelFilter;
use tracing::subscriber::Interest;

use crate::opentelclient::{AnyValue, ExportLogsServiceRequest, KeyValue, LogRecord, Resource, ResourceLogs, ScopeLogs};
use crate::opentelclient::any_value::Value::StringValue;
use crate::opentelclient::logs_service_client::LogsServiceClient;

mod opentelclient;
pub struct TelescopeSubscriber {
    tx: SyncSender<LogRecord>,
}

impl TelescopeSubscriber {
   async  fn new(client: LogsServiceClient<Channel>, service_name : String, url : String) -> Self {
       let url_leak = Box::leak(url.into_boxed_str());
        LogsServiceClient::new(
            Channel::from_static(url_leak)
                .connect()
                .await
                .unwrap());
        let (tx, rx) = sync_channel(1000);

        start_logging_thread(rx, client, service_name.clone());
        Self {
            tx
        }
    }
}

fn start_logging_thread(rx: Receiver<LogRecord>, mut client: LogsServiceClient<Channel>, service_name: String) {
    thread::spawn(move || {
        let mut buffer = Vec::with_capacity(1000);
        let mut last_send = Instant::now();
        let rt = tokio::runtime::Runtime::new().unwrap();
        loop {
            while let Ok(record) = rx.try_recv() {
                buffer.push(record);
                if buffer.len() == 1000 {
                    break;
                }
            }

                if buffer.len() >= 100 || last_send.elapsed().as_millis() >= 1000 {
                loop {
                    let logs = ResourceLogs {
                        resource: Some(Resource {
                            attributes: vec![KeyValue {
                                key: "service.name".to_string(),
                                value: Some(AnyValue {
                                    value: Some(StringValue(service_name.clone())),
                                }),
                            }],
                            dropped_attributes_count: 0,
                        }),
                        scope_logs: vec![ScopeLogs {
                            scope: None,
                            log_records: buffer.drain(..).collect(),
                            schema_url: "".to_string(),
                        }],
                        schema_url: "".to_string(),
                    };

                    let request = Request::new(ExportLogsServiceRequest {
                        resource_logs: vec![logs],
                    });

                    match rt.block_on(async { client.export(request).await }) {
                        Ok(_) => break, // If request succeeded, the loop is broken
                        Err(_) => {
                            thread::sleep(Duration::from_secs(1));
                        }
                    }
                }
                last_send = Instant::now();
            } else {
                // Allow thread to sleep for a while before next check
                thread::sleep(Duration::from_millis(50));
            }
        }
    });
}

impl Subscriber for TelescopeSubscriber {
    fn on_register_dispatch(&self, _: &Dispatch) {}

    fn register_callsite(&self, _: &'static Metadata<'static>) -> Interest {
        Interest::always()
    }

    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::TRACE)
    }

    fn new_span(&self, _span: &span::Attributes<'_>) -> span::Id {
        Id::from_u64(1)
    }

    fn record(&self, _span: &Id, _values: &span::Record<'_>) {
        // This method records updated values for a span.
    }

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {
        // This method records that a span follows from another span.
    }

    fn event_enabled(&self, _: &Event<'_>) -> bool {
        true
    }

    fn event(&self, event: &Event<'_>) {

        if event.metadata().level() == &Level::INFO
            || event.metadata().level() == &Level::WARN
            || event.metadata().level() == &Level::ERROR
        {

            // This method records that an event has occurred.
            let mut visitor = FieldVisitor {
                values: HashMap::new(),
            };
            event.record(&mut visitor);

            let unix_nano = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64;
            let body = visitor.values["message"].to_string();
            let record = LogRecord {
                time_unix_nano: unix_nano,
                observed_time_unix_nano: unix_nano,
                severity_number: match event.metadata().level() {
                    &Level::TRACE => 1,
                    &Level::DEBUG => 5,
                    &Level::INFO => 9,
                    &Level::WARN => 13,
                    &Level::ERROR => 17,
                },
                severity_text: event.metadata().level().to_string().clone(),
                body: Some(AnyValue {
                    value: Some(StringValue(body.clone())),
                }),
                attributes: vec![],
                dropped_attributes_count: 0,
                flags: 0,
                trace_id: vec![],
                span_id: vec![],
            };
            println!("{} {} {:?}", Local::now(), record.severity_text, body);
            self.tx.send(record).unwrap();
        }
    }

    fn enter(&self, _span: &Id) {
        // This method records that a span has been entered.
    }

    fn exit(&self, _span: &Id) {
        // This method records that a span has been exited.
    }
}

struct FieldVisitor {
    values: HashMap<String, String>,
}

impl tracing_core::field::Visit for FieldVisitor {
    // record primitives
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.values
            .insert(field.name().to_string(), format!("{:?}", value));
    }
}