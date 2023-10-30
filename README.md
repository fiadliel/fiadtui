[![Docs](https://img.shields.io/badge/crates.io-0.9.0-red)](https://crates.io/crates/fiadtui)
 [![Docs](https://img.shields.io/badge/docs.rs-blue)](https://docs.rs/fiadtui/latest/fiadtui/)

# fiadtui

Simple TUI wrapper for ratatui with tokio and crossterm.

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
