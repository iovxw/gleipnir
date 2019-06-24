#![feature(async_await)]
#![feature(proc_macro_hygiene)]

use std::cmp::min;
use std::fmt;
use std::mem;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::ops::RangeInclusive;

use libc;
use serde::{Deserialize, Serialize};

pub mod unixtransport;

pub mod daemon {
    use super::Rules;
    tarpc::service! {
        rpc init_monitor(socket_path: String);
        rpc unlock() -> bool;
        rpc set_rules(rules: Rules);
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Rules {
    pub default_target: RuleTarget,
    pub rules: Vec<Rule>,
    pub rate_rules: Vec<RateLimitRule>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RateLimitRule {
    pub name: String,
    pub limit: usize,
}

pub mod monitor {
    use super::*;
    tarpc::service! {
        rpc on_packages(logs: Vec<PackageReport>);
        rpc on_rules_updated(rules: Rules);
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackageReport {
    pub device: Device,
    pub protocol: Proto,
    pub addr: SocketAddr,
    pub len: usize,
    pub exe: String,
    pub dropped: bool,
    pub matched_rule: Option<usize>,
}

#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Device {
    Input,
    Output,
}

impl Device {
    pub fn is_input(&self) -> bool {
        match self {
            Device::Input => true,
            Device::Output => false,
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Proto {
    Tcp = libc::IPPROTO_TCP as isize,
    Udp = libc::IPPROTO_UDP as isize,
    UdpLite = libc::IPPROTO_UDPLITE as isize,
}

impl fmt::Display for Proto {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match *self {
            Proto::Tcp => "TCP",
            Proto::Udp => "UDP",
            Proto::UdpLite => "UDPLite",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum RuleTarget {
    Accept,
    Drop,
    RateLimit(usize), // index to rate_rules item
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub device: Option<Device>,
    pub proto: Option<Proto>,
    pub exe: Option<String>,
    #[serde(with = "rangeinclusive_serde")]
    pub port: Option<RangeInclusive<u16>>,
    pub subnet: (IpAddr, u8), // mask
    pub target: RuleTarget,
}

impl Rule {
    pub fn match_target(
        &self,
        device: Device,
        protocol: Proto,
        addr: SocketAddr,
        exe: &str,
    ) -> Option<RuleTarget> {
        if (self.device.is_none() || device == self.device.unwrap())
            && (self.proto.is_none() || protocol == self.proto.unwrap())
            && (self.exe.is_none() || exe == self.exe.as_ref().unwrap())
            && (self.port.is_none() || self.port.as_ref().unwrap().contains(&addr.port()))
            && addr.is_ipv4() == self.subnet.0.is_ipv4()
            && (match (addr.ip(), self.subnet) {
                (IpAddr::V4(addr), (IpAddr::V4(subnet), mask)) => addr.mask(mask) == subnet,
                (IpAddr::V6(addr), (IpAddr::V6(subnet), mask)) => addr.mask(mask) == subnet,
                _ => unreachable!(),
            })
        {
            Some(self.target)
        } else {
            None
        }
    }
}

mod rangeinclusive_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::ops::RangeInclusive;

    #[derive(Serialize)]
    pub struct RangeInclusiveRef<'a, Idx> {
        start: &'a Idx,
        end: &'a Idx,
    }
    #[derive(Deserialize)]
    pub struct RangeInclusiveOwned<Idx> {
        start: Idx,
        end: Idx,
    }

    pub fn serialize<'a, S, Idx: 'a>(
        this: &Option<RangeInclusive<Idx>>,
        ser: S,
    ) -> Result<S::Ok, S::Error>
    where
        Idx: Serialize,
        S: Serializer,
    {
        let r = this.as_ref().map(|range| RangeInclusiveRef {
            start: range.start(),
            end: range.end(),
        });
        <Option<RangeInclusiveRef<Idx>> as Serialize>::serialize(&r, ser)
    }

    pub fn deserialize<'de, D, Idx>(de: D) -> Result<Option<RangeInclusive<Idx>>, D::Error>
    where
        Idx: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        let r = <Option<RangeInclusiveOwned<Idx>> as Deserialize>::deserialize(de)?
            .map(|range| RangeInclusive::new(range.start, range.end));
        Ok(r)
    }
}

pub trait Address: Copy {
    type Nibbles: AsRef<[u8]>;
    /// Convert to string of nibbles.
    fn nibbles(self) -> Self::Nibbles;
    /// Convert from string of nibbles.
    fn from_nibbles(nibbles: &[u8]) -> Self;
    /// Returns self masked to n bits.
    fn mask(self, masklen: u8) -> Self;
}

impl Address for Ipv4Addr {
    type Nibbles = [u8; 8];

    fn nibbles(self) -> Self::Nibbles {
        let mut ret: Self::Nibbles = unsafe { mem::uninitialized() };
        let bytes: [u8; 4] = unsafe { mem::transmute(self) };
        for (i, byte) in bytes.iter().enumerate() {
            ret[i * 2] = byte >> 4;
            ret[i * 2 + 1] = byte & 0xf;
        }
        ret
    }

    fn from_nibbles(nibbles: &[u8]) -> Self {
        let mut ret: [u8; 4] = [0; 4];
        let lim = min(ret.len() * 2, nibbles.len());
        for (i, nibble) in nibbles.iter().enumerate().take(lim) {
            match i % 2 {
                0 => {
                    ret[i / 2] = *nibble << 4;
                }
                _ => {
                    ret[i / 2] |= *nibble;
                }
            }
        }
        unsafe { mem::transmute(ret) }
    }

    fn mask(self, masklen: u8) -> Self {
        debug_assert!(masklen <= 32);
        let ip = u32::from(self);
        let masked = match masklen {
            0 => 0,
            n => ip & (!0 << (32 - n)),
        };
        Ipv4Addr::from(masked)
    }
}

impl Address for Ipv6Addr {
    type Nibbles = [u8; 32];

    fn nibbles(self) -> Self::Nibbles {
        let mut ret: Self::Nibbles = unsafe { mem::uninitialized() };
        let bytes: [u8; 16] = unsafe { mem::transmute(self) };
        for (i, byte) in bytes.iter().enumerate() {
            ret[i * 2] = byte >> 4;
            ret[i * 2 + 1] = byte & 0xf;
        }
        ret
    }

    fn from_nibbles(nibbles: &[u8]) -> Self {
        let mut ret: [u8; 16] = [0; 16];
        let lim = min(ret.len() * 2, nibbles.len());
        for (i, nibble) in nibbles.iter().enumerate().take(lim) {
            match i % 2 {
                0 => {
                    ret[i / 2] = *nibble << 4;
                }
                _ => {
                    ret[i / 2] |= *nibble;
                }
            }
        }
        unsafe { mem::transmute(ret) }
    }

    fn mask(self, masklen: u8) -> Self {
        debug_assert!(masklen <= 128);
        let (first, last): (u64, u64) = unsafe { mem::transmute(self) };
        if masklen <= 64 {
            let masked = match masklen {
                0 => 0,
                n => first.to_be() & (!0 << (64 - n)),
            };
            unsafe { mem::transmute((masked.to_be(), 0u64)) }
        } else {
            let masked = match masklen {
                64 => 0,
                n => last.to_be() & (!0 << (128 - n)),
            };
            unsafe { mem::transmute((first, masked.to_be())) }
        }
    }
}
