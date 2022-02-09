use std::{
    fmt::Display,
    net::Ipv4Addr,
    time::{Duration, Instant},
};

use icmp_socket::{packet::WithEchoRequest, IcmpSocket, IcmpSocket4, Icmpv4Message, Icmpv4Packet};

#[derive(Debug)]
pub enum Error {
    Unreachable,
    IoError(std::io::Error),
    SendError,
    UnknownReturnAddress(Ipv4Addr),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unreachable => write!(f, "Unreachable"),
            Error::IoError(error) => write!(f, "{}", error),
            Error::SendError => write!(f, "Send Error"),
            Error::UnknownReturnAddress(ipv4) => write!(f, "Unknown Return Address {}", ipv4),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::IoError(error)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PingInfo {
    rtt: Duration,
}

impl PingInfo {
    pub fn new(rtt: Duration) -> Self {
        Self { rtt }
    }

    /// Get ping info's rtt.
    pub fn get_rtt(&self) -> Duration {
        self.rtt
    }
}

impl std::fmt::Display for PingInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.2} ms", self.rtt.as_secs_f64() * 1000.0)
    }
}

pub struct Pinger {
    socket: IcmpSocket4,

    timeout: Duration,
}

impl Pinger {
    pub fn new() -> Self {
        let mut socket = IcmpSocket4::new().unwrap();
        socket.bind("0.0.0.0".parse::<Ipv4Addr>().unwrap()).unwrap();
        Self {
            socket,
            timeout: Duration::from_secs(2),
        }
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub fn ping(&mut self, ipv4: impl Into<Ipv4Addr>, sequence: u16) -> Result<PingInfo, Error> {
        let ipv4 = ipv4.into();
        let packet = Icmpv4Packet::with_echo_request(
            42,
            sequence,
            vec![
                0x20, 0x20, 0x75, 0x73, 0x74, 0x20, 0x61, 0x20, 0x66, 0x6c, 0x65, 0x73, 0x68, 0x20,
                0x77, 0x6f, 0x75, 0x6e, 0x64, 0x20, 0x20, 0x74, 0x69, 0x73, 0x20, 0x62, 0x75, 0x74,
                0x20, 0x61, 0x20, 0x73, 0x63, 0x72, 0x61, 0x74, 0x63, 0x68, 0x20, 0x20, 0x6b, 0x6e,
                0x69, 0x67, 0x68, 0x74, 0x73, 0x20, 0x6f, 0x66, 0x20, 0x6e, 0x69, 0x20, 0x20, 0x20,
            ],
        )
        .unwrap();

        let send_time = Instant::now();
        self.socket
            .send_to(ipv4, packet)
            .map_err(|_| Error::SendError)?;

        self.socket.set_timeout(Some(self.timeout));

        self.socket
            .rcv_from()
            .map_err(|error| error.into())
            .and_then(|(packet, address)| {
                let address = *address.as_socket_ipv4().unwrap().ip();
                if address == ipv4 {
                    Ok(packet)
                } else {
                    Err(Error::UnknownReturnAddress(address))
                }
            })
            .and_then(|packet| {
                if let Icmpv4Message::EchoReply { .. } = packet.message {
                    Ok(PingInfo::new(send_time.elapsed()))
                } else {
                    Err(Error::Unreachable)
                }
            })
    }
}

impl Default for Pinger {
    fn default() -> Self {
        Self::new()
    }
}
