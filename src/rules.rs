use std::cell::RefCell;
use std::cmp::min;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::mem;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::ops::RangeInclusive;
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
#[derive(Clone)]
struct Rule {
    device: Option<Device>,
    proto: Option<Proto>,
    exe: Option<String>,
    port: Option<RangeInclusive<u16>>,
    subnet: (IpAddr, u32), // mask
    target: RuleTarget,
}

impl Rule {
    fn match_target(
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

pub struct Rules {
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
    port: HashMap<u16, Vec<usize>>,
    any_port: Vec<usize>,
    raw: Vec<Rule>,
    default_target: RuleTarget,
}

impl Rules {
    pub fn is_acceptable(
        &self,
        device: Device,
        protocol: Proto,
        addr: SocketAddr,
        len: usize,
        exe: &str,
    ) -> bool {
        let mut hasher = DefaultHasher::new();
        (device, protocol, addr, exe).hash(&mut hasher);
        let lru_index = hasher.finish();

        let target = MATCH_CACHE
            .with(|cache| cache.borrow_mut().get(&lru_index).cloned())
            .unwrap_or_else(|| {
                let target = self.match_target(device, protocol, addr, exe);
                MATCH_CACHE.with(|cache| cache.borrow_mut().insert(lru_index, target));
                target
            });

        target.is_acceptable(len)
    }

    fn match_target(
        &self,
        device: Device,
        protocol: Proto,
        addr: SocketAddr,
        exe: &str,
    ) -> RuleTarget {
        let empty = Vec::new();
        let exact_device = self.device.get(&device).unwrap_or(&empty);
        let exact_proto = self.proto.get(&protocol).unwrap_or(&empty);
        let exact_exe = self.exe.get(exe).unwrap_or(&empty);
        let exact_port = self.port.get(&addr.port()).unwrap_or(&empty);
        let (exact_ip, any_ip) = match addr.ip() {
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
            (exact_port, &self.any_port),
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
            .unwrap_or(self.default_target)
    }
}

macro_rules! insert_rule {
    ($target: tt, $rule: tt, $name: tt, $any: tt,  $index: tt) => {
        if let Some(k) = $rule.$name {
            $target.$name.entry(k).or_default().push($index);
        } else {
            $target.$any.push($index);
        }
    };
}

impl From<Vec<Rule>> for Rules {
    fn from(rules: Vec<Rule>) -> Self {
        let mut r = Self {
            device: Default::default(),
            any_device: Default::default(),
            proto: Default::default(),
            any_proto: Default::default(),
            exe: Default::default(),
            any_exe: Default::default(),
            v4_table: IpLookupTable::new(),
            any_v4: Default::default(),
            v6_table: IpLookupTable::new(),
            any_v6: Default::default(),
            port: Default::default(),
            any_port: Default::default(),
            raw: rules[1..].to_vec(),
            default_target: RuleTarget::Accept,
        };
        let mut rules = rules.into_iter();
        // The default rule is rules[0]
        let default_rule = rules.next().expect("");

        // Other fields of default rule must be empty
        debug_assert!(default_rule.device.is_none());
        debug_assert!(default_rule.proto.is_none());
        debug_assert!(default_rule.exe.is_none());
        debug_assert!(default_rule.port.is_none());
        debug_assert!(default_rule.subnet.0.is_unspecified());
        debug_assert_eq!(default_rule.subnet.1, 0);

        r.default_target = default_rule.target;

        let mut v4_hashmap: HashMap<(Ipv4Addr, u32), Vec<usize>> = HashMap::new();
        let mut v6_hashmap: HashMap<(Ipv6Addr, u32), Vec<usize>> = HashMap::new();

        for (index, rule) in rules.enumerate() {
            insert_rule!(r, rule, device, any_device, index);
            insert_rule!(r, rule, proto, any_proto, index);
            insert_rule!(r, rule, exe, any_exe, index);
            if let Some(port_range) = rule.port {
                for port in port_range {
                    r.port.entry(port).or_default().push(index);
                }
            } else {
                r.any_port.push(index);
            }
            match rule.subnet {
                (IpAddr::V4(subnet), mask) => {
                    v4_hashmap.entry((subnet, mask)).or_default().push(index);
                }
                (IpAddr::V6(subnet), mask) => {
                    v6_hashmap.entry((subnet, mask)).or_default().push(index);
                }
            }
        }

        for ((ip, masklen), index) in v4_hashmap {
            r.v4_table.insert(ip, masklen, index);
        }
        for ((ip, masklen), index) in v6_hashmap {
            r.v6_table.insert(ip, masklen, index);
        }

        r
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
