[![Docs](https://img.shields.io/badge/crates.io-0.9.0-red)](https://crates.io/crates/fiadtui)
 [![Docs](https://img.shields.io/badge/docs.rs-blue)](https://docs.rs/fiadtui/latest/fiadtui/)

# fiadtui

Simple TUI wrapper for ratatui with tokio and crossterm.

## Features

- Maps low level events to application-level messages.
- Handles `^C` (quit), `^R` (refresh), `^Z` (suspend), leaves all other events to the user to configure.
- Message handlers may optionally return a future that outputs an application message. If so, this is run and handled by the event loop.
- Refresh rate is configured when the event loop starts. The refresh rate setting only modifies the drawing rate, not other event handling.
- An MPSC channel may be passed in when creating the event loop, allowing external events to be injected.
- Otherwise, the `draw()` function leaves organisation of how the frame is drawn to the user.

## Example usage

### Counter application

The classic counter (press `+` to increment, `-` to decrement).

[Link](examples/counter.rs)

```bash
cargo run --example counter
```

### Delayed counter application

Demonstrates the use of asynchronous message handlers.

Similar to the counter application, but updates the counter value
asynchronously after 1 second. Further updates are dropped while
any existing update is pending.

[Link](examples/delayed_counter.rs)

```bash
cargo run --example delayed_counter
```

### External tick application

Demonstrates the use of a channel created externally.

A `Tick` message is sent every 500ms to the app, from external code.

[Link](examples/tick.rs)

```bash
cargo run --example tick
```
