//! Possible modes of creating a data channel.
//!
//! Only the passive mode is implemented, although the active mode is required by
//! [RFC959](https://datatracker.ietf.org/doc/html/rfc959).

#[allow(unused)]
#[derive(Copy, Clone, Debug, Default)]
pub(crate) enum ConnectionMode {
  /// In active mode the server initiates a connection to address specified by client.
  Active,
  #[default]
  /// In passive mode the server only listens on a port of it's choosing.
  Passive,
}
