//! Transport-level device discovery scanners.
//!
//! Protocol-neutral scans that capture raw evidence about attached
//! devices: serial port output, mDNS service announcements and
//! reachable TCP hosts on a subnet. Interpreting the evidence —
//! deciding which driver could talk to a device — lives with the
//! embedding application's drivers, never here.

pub mod mdns;
pub mod serial;
pub mod subnet;
pub mod types;
