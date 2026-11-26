//! The Ruida packet transport and ACK handshake.
//!
//! This layer mirrors ``ruida_pa.ruidadriver.rd_transport``. USB links
//! carry raw swizzled bytes and are not acknowledged. UDP and TCP
//! links acknowledge every packet with a single byte; query packets
//! are followed by one or more replies.
//!
//! The packet state machine is explicit:
//!
//! ```text
//! Idle -> Send
//! Send -> USB: ReplyPending if query else Idle
//! Send -> UDP/TCP: AckPending
//! AckPending -> ACK: ReplyPending if query else Idle
//! AckPending -> reject byte: learn magic and retry once, else fail
//! AckPending -> timeout: Timeout
//! ReplyPending -> replies arrived: Idle
//! ReplyPending -> ACKed but no reply: Busy
//! ```

use std::time::Duration;

use crate::core::driver::DriverError;
use crate::ruida::codec;
use crate::ruida::link::timed_out;
pub use crate::ruida::link::{open_link, Link, TransportKind};
use crate::ruida::protocol::{ERR, NAK};

/// How a transport frames a command packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Framing {
    Raw,
    UdpChecksum,
}

/// Whether a transport acknowledges packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AckMode {
    None,
    SingleByte,
}

/// The wire behavior of one transport.
#[derive(Debug, Clone, Copy)]
struct WireProfile {
    framing: Framing,
    ack: AckMode,
    learn_magic: bool,
}

impl WireProfile {
    fn for_kind(kind: TransportKind) -> Self {
        match kind {
            TransportKind::Usb => Self {
                framing: Framing::Raw,
                ack: AckMode::None,
                learn_magic: false,
            },
            TransportKind::Udp => Self {
                framing: Framing::UdpChecksum,
                ack: AckMode::SingleByte,
                learn_magic: true,
            },
            TransportKind::Tcp => Self {
                framing: Framing::Raw,
                ack: AckMode::SingleByte,
                learn_magic: true,
            },
        }
    }

    fn frames_with_checksum(self) -> bool {
        self.framing == Framing::UdpChecksum
    }
}

/// One state of the per-packet handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PacketState {
    Idle,
    Send,
    AckPending,
    ReplyPending,
}

/// The result of a query packet.
#[derive(Debug)]
pub(crate) enum QueryOutcome {
    Replied(Vec<codec::Reply>),
    /// The controller acknowledged the packet but returned no reply.
    Busy,
}

/// A link plus the Ruida packet handshake.
pub struct RuidaTransport {
    link: Box<dyn Link>,
    kind: TransportKind,
    profile: WireProfile,
    magic: u8,
    ack_table: [u8; 256],
    rx: Vec<u8>,
    replies: Vec<codec::Reply>,
    pub timeout: Duration,
    pub inter_packet_timeout: Duration,
}

impl RuidaTransport {
    /// Wraps a link. ``magic`` is the controller's swizzle key.
    pub fn new(link: Box<dyn Link>, kind: TransportKind, magic: u8) -> Self {
        Self {
            link,
            kind,
            profile: WireProfile::for_kind(kind),
            magic,
            ack_table: codec::ack_magic_table(),
            rx: Vec::new(),
            replies: Vec::new(),
            timeout: Duration::from_millis(500),
            inter_packet_timeout: Duration::from_millis(50),
        }
    }

    pub fn kind(&self) -> TransportKind {
        self.kind
    }

    pub fn magic(&self) -> u8 {
        self.magic
    }

    /// Sends raw (unswizzled) command bytes, chunking them into
    /// packets and acknowledging each one. No replies are expected.
    pub async fn send(&mut self, raw: &[u8]) -> Result<(), DriverError> {
        for packet in self.packets(raw) {
            self.run_packet(&packet, false).await?;
        }
        Ok(())
    }

    /// Sends one query packet and returns its replies.
    pub async fn query(
        &mut self,
        raw: &[u8],
    ) -> Result<Vec<codec::Reply>, DriverError> {
        match self.query_outcome(raw).await? {
            QueryOutcome::Replied(replies) => Ok(replies),
            QueryOutcome::Busy => Err(DriverError::Timeout(
                "controller acknowledged the query but did not reply"
                    .to_string(),
            )),
        }
    }

    /// Sends one query packet and distinguishes Busy from failure.
    pub(crate) async fn query_outcome(
        &mut self,
        raw: &[u8],
    ) -> Result<QueryOutcome, DriverError> {
        let packet = self.pack(raw);
        match self.run_packet(&packet, true).await? {
            true => {
                Ok(QueryOutcome::Replied(std::mem::take(&mut self.replies)))
            }
            false => Ok(QueryOutcome::Busy),
        }
    }

    fn packets(&self, raw: &[u8]) -> Vec<Vec<u8>> {
        const CHUNK_SIZE: usize = 1024;
        raw.chunks(CHUNK_SIZE)
            .map(|chunk| self.pack(chunk))
            .collect()
    }

    fn pack(&self, raw: &[u8]) -> Vec<u8> {
        codec::package(raw, self.magic, self.profile.frames_with_checksum())
    }

    /// Runs one packet through the handshake state machine.
    ///
    /// Returns ``true`` when a query produced replies and ``false``
    /// when it was acknowledged without a reply (Busy).
    async fn run_packet(
        &mut self,
        packet: &[u8],
        expects_replies: bool,
    ) -> Result<bool, DriverError> {
        let mut state = PacketState::Send;
        let mut magic_retried = false;
        loop {
            match state {
                PacketState::Idle => return Ok(false),
                PacketState::Send => {
                    self.link.write(packet).await?;
                    state = if self.profile.ack == AckMode::None {
                        if expects_replies {
                            PacketState::ReplyPending
                        } else {
                            PacketState::Idle
                        }
                    } else {
                        PacketState::AckPending
                    };
                }
                PacketState::AckPending => {
                    let Some(data) = self.read_unit(self.timeout).await? else {
                        return Err(timed_out());
                    };
                    let Some(first) = data.first().copied() else {
                        return Err(timed_out());
                    };
                    if codec::unswizzle_byte(first, self.magic) == codec::ACK {
                        if data.len() > 1 {
                            self.rx.extend_from_slice(&data[1..]);
                        }
                        state = if expects_replies {
                            PacketState::ReplyPending
                        } else {
                            PacketState::Idle
                        };
                        continue;
                    }
                    if data.len() == 1
                        && self.profile.learn_magic
                        && !magic_retried
                    {
                        let magic = self.ack_table[usize::from(first)];
                        if magic != 0 {
                            self.magic = magic;
                            magic_retried = true;
                            state = PacketState::Send;
                            continue;
                        }
                    }
                    if data.len() > 1 {
                        self.rx.extend_from_slice(&data[1..]);
                    }
                    let code = codec::unswizzle_byte(first, self.magic);
                    let detail = match code {
                        ERR => "controller reported an error",
                        NAK => "controller reported a checksum error",
                        _ => "controller rejected the packet",
                    };
                    return Err(DriverError::Connection(format!(
                        "{detail} (0x{code:02X})"
                    )));
                }
                PacketState::ReplyPending => {
                    let count = self.collect_replies().await?;
                    return Ok(count > 0);
                }
            }
        }
    }

    /// Reads replies until no more data arrives within the
    /// inter-packet gap.
    async fn collect_replies(&mut self) -> Result<usize, DriverError> {
        let Some(mut data) = self.read_unit(self.timeout).await? else {
            return Ok(0);
        };
        loop {
            match self.link.read(self.inter_packet_timeout).await {
                Ok(Some(more)) => data.extend(more),
                Ok(None) => break,
                Err(error) => return Err(error),
            }
        }
        let raw = codec::unswizzle(&data, self.magic);
        let replies = codec::unpack_replies(&raw);
        let count = replies.len();
        self.replies.extend(replies);
        Ok(count)
    }

    async fn read_unit(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Vec<u8>>, DriverError> {
        if !self.rx.is_empty() {
            return Ok(Some(std::mem::take(&mut self.rx)));
        }
        self.link.read(timeout).await
    }

    /// Closes the underlying link.
    pub async fn close(&mut self) {
        self.link.close().await;
        self.rx.clear();
    }
}
