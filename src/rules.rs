use std::cell::RefCell;
use std::cmp::min;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::mem;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{Duration, Instant};

use lru_time_cache::LruCache;
use treebitmap::IpLookupTable;

use crate::netlink::Proto;
use crate::Device;

thread_local! {
    static QOS_STATE: RefCell<Vec<Bucket>> = RefCell::new(Vec::new());
    static MATCH_CACHE: RefCell<LruCache<u64, RuleTarget>> =
        RefCell::new(LruCache::with_capacity(2048));
}

// When Rules changed, call this
pub fn refresh_local(qos_rules: Vec<usize>) {
    QOS_STATE.with(|rules| {
        let mut rules = rules.borrow_mut();
        rules.clear();
        for limit in qos_rules {
            rules.push(Bucket::new(limit));
        }
    });
    MATCH_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
}

struct Bucket {
    bytes: usize,
    timestamp: Instant,
    limit: usize,
}

impl Bucket {
    fn new(limit: usize) -> Self {
        Self {
            bytes: 0,
            timestamp: Instant::now(),
            limit,
        }
    }
    pub fn stuff(&mut self, size: usize) -> bool {
        if self.bytes() + size < self.limit {
            self.bytes += size;
            true
        } else {
            false
        }
    }

    pub fn bytes(&mut self) -> usize {
        const PERIOD: Duration = Duration::from_millis(500);
        let now = Instant::now();
        if self.timestamp + PERIOD >= now {
            self.timestamp = now;
            self.bytes = 0;
        }
        self.bytes
    }
}

// dbus
struct QosRules(Vec<usize>);

// impl From<QosRules> for QOS_STATE

// dbus
struct DbusRules(Vec<Rule>);

// impl From<DbusRules> for Rules {}

// dbus
#[derive(Debug, Copy, Clone)]
enum RuleTarget {
    Accept,
    Drop,
    Qos(usize), // index to QosRules
}

impl RuleTarget {
    fn is_acceptable(&self, pkt_size: usize) -> bool {
        use RuleTarget::*;
        match *self {
            Accept => true,
            Drop => false,
            Qos(qos_id) => QOS_STATE.with(|rules| rules.borrow_mut()[qos_id].stuff(pkt_size)),
        }
    }
}

// dbus
struct Rule {
    device: Option<Device>,
    proto: Option<Proto>,
    exe: Option<String>,
    v4: (Ipv4Addr, u32), // mask
    v6: (Ipv6Addr, u32),
    target: RuleTarget,
}

impl Rule {
    fn match_target(
        &self,
        device: Device,
        protocol: Proto,
        addr: IpAddr,
        exe: &str,
    ) -> Option<RuleTarget> {
        if (self.device.is_none() || device == self.device.unwrap())
            && (self.proto.is_none() || protocol == self.proto.unwrap())
            && (self.exe.is_none() || exe == self.exe.as_ref().unwrap())
            && (match addr {
                IpAddr::V4(addr) => addr.mask(self.v4.1) == self.v4.0,
                IpAddr::V6(addr) => addr.mask(self.v6.1) == self.v6.0,
            })
        {
            Some(self.target)
        } else {
            None
        }
    }
}

struct Rules {
    device: HashMap<Device, Vec<usize>>,
    any_device: Vec<usize>,
    proto: HashMap<Proto, Vec<usize>>,
    any_proto: Vec<usize>,
    exe: HashMap<String, Vec<usize>>,
    any_exe: Vec<usize>,
    v4_table: IpLookupTable<Ipv4Addr, Vec<usize>>,
    any_v4: Vec<usize>,
    v6_table: IpLookupTable<Ipv6Addr, Vec<usize>>,
    any_v6: Vec<usize>,
    raw: Vec<Rule>,
}

impl Rules {
    pub fn is_acceptable(
        &mut self,
        device: Device,
        protocol: Proto,
        addr: IpAddr,
        len: usize,
        exe: &str,
    ) -> Option<bool> {
        let mut hasher = DefaultHasher::new();
        (device, protocol, addr, exe).hash(&mut hasher);
        let lru_index = hasher.finish();
        MATCH_CACHE
            .with(|cache| cache.borrow_mut().get(&lru_index).cloned())
            .or_else(|| {
                self.match_target(device, protocol, addr, exe).map(|r| {
                    MATCH_CACHE.with(|cache| cache.borrow_mut().insert(lru_index, r));
                    r
                })
            })
            .map(|target| target.is_acceptable(len))
    }
    fn match_target(
        &self,
        device: Device,
        protocol: Proto,
        addr: IpAddr,
        exe: &str,
    ) -> Option<RuleTarget> {
        let empty = Vec::new();
        let exact_device = self.device.get(&device).unwrap_or(&empty);
        let exact_proto = self.proto.get(&protocol).unwrap_or(&empty);
        let exact_exe = self.exe.get(exe).unwrap_or(&empty);
        let (exact_ip, any_ip) = match addr {
            IpAddr::V4(ip) => (
                self.v4_table
                    .longest_match(ip)
                    .map(|x| x.2)
                    .unwrap_or(&empty),
                &self.any_v4,
            ),
            IpAddr::V6(ip) => (
                self.v6_table
                    .longest_match(ip)
                    .map(|x| x.2)
                    .unwrap_or(&empty),
                &self.any_v6,
            ),
        };
        let list = [
            (exact_device, &self.any_device),
            (exact_proto, &self.any_proto),
            (exact_exe, &self.any_exe),
            (exact_ip, any_ip),
        ];
        let (exact, any) = list
            .iter()
            .min_by_key(|(exact, any)| exact.len() + any.len())
            .unwrap();

        exact
            .into_iter()
            .chain(*any)
            .filter_map(|&id| {
                self.raw[id]
                    .match_target(device, protocol, addr, exe)
                    .map(|t| (id, t))
            })
            .min_by_key(|(id, _)| *id)
            .map(|(_, t)| t)
    }
}

pub trait Address: Copy {
    type Nibbles: AsRef<[u8]>;
    /// Convert to string of nibbles.
    fn nibbles(self) -> Self::Nibbles;
    /// Convert from string of nibbles.
    fn from_nibbles(nibbles: &[u8]) -> Self;
    /// Returns self masked to n bits.
    fn mask(self, masklen: u32) -> Self;
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

    fn mask(self, masklen: u32) -> Self {
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

    fn mask(self, masklen: u32) -> Self {
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
