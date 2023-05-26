//! Contains implementation of handlers and reply sender.
//!
//! Each handler represents a single connection from client. Handlers listen for commands from
//! clients in a loop and send the commands for processing.
//!
//! Reply sender is generic for each protocol. It is used for sending replies back to client.
pub(crate) mod connection_handler;
pub(crate) mod quic_only_connection_handler;
pub(crate) mod reply_sender;
pub(crate) mod standard_connection_handler;
pub(crate) mod standard_tls_connection_handler;
