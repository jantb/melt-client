``` rust
let fmt_subscriber = tracing_subscriber::fmt()
// Use a more compact, abbreviated log format
.compact()
// Display source code file paths
.with_file(true)
// Display source code line numbers
.with_line_number(true)
// Display the thread ID an event was recorded on
.with_thread_ids(true)
// Don't display the event's target (module path)
.with_target(false)
// Build the subscriber
.finish();
let telescope_layer = TelescopeLayer::new("telescope".to_string(), "http://0.0.0.0:4317".to_string()).await;

set_global_default(
    fmt_subscriber.with(telescope_layer)
)
.expect("sets the global default subscriber");
```