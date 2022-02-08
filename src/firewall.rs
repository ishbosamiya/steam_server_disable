use std::{fmt::Display, net::Ipv4Addr};

#[derive(Debug)]
pub enum Error {
    UnsuccessfulBlockCheck(Ipv4Addr),
    UnsuccessfulBan(Ipv4Addr),
    UnsuccessfulUnban(Ipv4Addr),
    Custom(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnsuccessfulBlockCheck(ip) => {
                write!(f, "Unsuccessful block check for {}", ip)
            }
            Error::UnsuccessfulBan(ip) => {
                write!(f, "Unsuccessful ban for {}", ip)
            }
            Error::UnsuccessfulUnban(ip) => {
                write!(f, "Unsuccessful unban for {}", ip)
            }
            Error::Custom(string) => write!(f, "{}", string),
        }
    }
}

impl std::error::Error for Error {}

trait FirewallRequirements: Default {
    /// Checks if ip exists in the firewall and thus is blocked
    fn is_blocked(&self, ip: Ipv4Addr) -> Result<bool, Error>;

    /// Ban the ip by adding it to the firewall
    fn ban_ip(&self, ip: Ipv4Addr) -> Result<(), Error>;

    /// Unban the ip by removing it from the firewall if it was
    /// blocked previously
    fn unban_ip(&self, ip: Ipv4Addr) -> Result<(), Error>;
}

pub struct Firewall {
    #[cfg(unix)]
    unix_firewall: unix::Firewall,
    #[cfg(windows)]
    windows_firewall: windows::Firewall,
}

impl Default for Firewall {
    fn default() -> Self {
        Self::new()
    }
}

impl Firewall {
    pub fn new() -> Self {
        Self {
            #[cfg(unix)]
            unix_firewall: unix::Firewall::default(),
            #[cfg(windows)]
            windows_firewall: windows::Firewall::default(),
        }
    }

    pub fn is_blocked(&self, ip: Ipv4Addr) -> Result<bool, Error> {
        #[cfg(unix)]
        {
            self.unix_firewall.is_blocked(ip)
        }
        #[cfg(windows)]
        {
            self.windows_firewall.is_blocked(ip)
        }
    }

    pub fn ban_ip(&self, ip: Ipv4Addr) -> Result<(), Error> {
        #[cfg(unix)]
        {
            self.unix_firewall.ban_ip(ip)
        }
        #[cfg(windows)]
        {
            self.windows_firewall.ban_ip(ip)
        }
    }

    pub fn unban_ip(&self, ip: Ipv4Addr) -> Result<(), Error> {
        #[cfg(unix)]
        {
            self.unix_firewall.unban_ip(ip)
        }
        #[cfg(windows)]
        {
            self.windows_firewall.unban_ip(ip)
        }
    }
}

#[cfg(unix)]
mod unix {
    use super::{Error, FirewallRequirements};

    pub struct Firewall {
        ipt: iptables::IPTables,
    }

    impl Firewall {
        pub fn new() -> Self {
            Self {
                ipt: iptables::new(false).unwrap(),
            }
        }
    }

    impl Default for Firewall {
        fn default() -> Self {
            Self::new()
        }
    }

    impl FirewallRequirements for Firewall {
        fn is_blocked(&self, ip: std::net::Ipv4Addr) -> Result<bool, Error> {
            let rule = format!("-s {} -j DROP", ip);
            self.ipt
                .exists("filter", "INPUT", &rule)
                .map_err(|_| Error::UnsuccessfulBlockCheck(ip))
        }

        fn ban_ip(&self, ip: std::net::Ipv4Addr) -> Result<(), Error> {
            let rule = format!("-s {} -j DROP", ip);
            self.ipt
                .append_replace("filter", "INPUT", &rule)
                .map_err(|_| Error::UnsuccessfulBan(ip))
        }

        fn unban_ip(&self, ip: std::net::Ipv4Addr) -> Result<(), Error> {
            let rule = format!("-s {} -j DROP", ip);
            self.ipt
                .delete_all("filter", "INPUT", &rule)
                .map_err(|_| Error::UnsuccessfulUnban(ip))
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::process::Command;

    use super::{Error, FirewallRequirements};

    pub struct Firewall {}

    impl Default for Firewall {
        fn default() -> Self {
            Self {}
        }
    }

    impl FirewallRequirements for Firewall {
        fn is_blocked(&self, ip: std::net::Ipv4Addr) -> Result<bool, Error> {
            let output = Command::new("netsh")
                .arg("advfirewall")
                .arg("firewall")
                .arg("show")
                .arg("rule")
                .arg(format!("name=\"IP_BLOCK_{}\"", ip))
                .output()
                .unwrap();
            Ok(output.status.success())
        }

        fn ban_ip(&self, ip: std::net::Ipv4Addr) -> Result<(), Error> {
            let output = Command::new("netsh")
                .arg("advfirewall")
                .arg("firewall")
                .arg("add")
                .arg("rule")
                .arg(format!("name=\"IP_BLOCK_{}\"", ip))
                .arg("dir=out")
                .arg("interface=any")
                .arg("action=block")
                .arg(format!("remoteip={}/32", ip))
                .output()
                .unwrap();
            if !output.status.success() {
                Err(Error::UnsuccessfulBan(ip))
            } else {
                Ok(())
            }
        }

        fn unban_ip(&self, ip: std::net::Ipv4Addr) -> Result<(), Error> {
            let output = Command::new("netsh")
                .arg("advfirewall")
                .arg("firewall")
                .arg("delete")
                .arg("rule")
                .arg(format!("name=\"IP_BLOCK_{}\"", ip))
                .output()
                .unwrap();
            if !output.status.success() {
                Err(Error::UnsuccessfulUnban(ip))
            } else {
                Ok(())
            }
        }
    }
}
