//! Epilog laser support: job submission over the BSD LPD protocol.
//!
//! Epilog laser cutters (Zing 16, Zing 24/6030, and the wider family
//! that shares the print driver) are print devices: the host uploads
//! a PJL/PCL/HPGL print job to the machine's network print queue and
//! the operator starts it at the control panel. There is no status
//! channel, no homing, jogging or power control over the network.
//!
//! The transfer is the classic BSD "receive job" exchange on
//! port 515:
//!
//! ```text
//! -> \x02 <LF>                              receive a job
//! <- \x00
//! -> \x02 <len> cfA<n>rayforge <LF>          control file header
//! <- \x00
//! -> <control file bytes> <NUL>              (H/P/J/l/U/N lines)
//! <- \x00
//! -> \x03 <len> dfA<n>rayforge <LF>          data file header
//! <- \x00
//! -> <payload bytes>                         the print job
//! <- \x00
//! ```
//!
//! Every stage must be acknowledged with a single `\x00` byte. A
//! non-zero reply is a rejection; a short read is a disconnect. A
//! partial transfer is never retried, because the machine may have
//! queued a truncated job.
//!
//! Jobs are handed to the session as the encoder's byte stream (see
//! the `upload` extra rather than the G-code `run` path).

pub mod session;
