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
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum RuleTarget {
    Accept,
    Drop,
    Qos(usize), // index to QosRules
}

// dbus
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rule {
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
    // TODO: better memory usage
    port: HashMap<u16, Vec<usize>>,
    any_port: Vec<usize>,
    raw: Vec<Rule>,
    default_target: RuleTarget,
    qos_state: RefCell<Vec<Bucket>>,
    cache: RefCell<LruCache<u64, (Option<usize>, RuleTarget)>>,
}

impl Rules {
    pub fn new(default_target: RuleTarget, rules: Vec<Rule>, qos_rules: Vec<usize>) -> Self {
        macro_rules! insert_rule {
            ($target: tt, $rule: tt, $name: tt, $any: tt,  $index: tt) => {
                if let Some(k) = $rule.$name {
                    $target.$name.entry(k).or_default().push($index);
                } else {
                    $target.$any.push($index);
                }
            };
        }

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
            raw: rules.clone(),
            default_target: default_target,
            qos_state: Default::default(),
            cache: RefCell::new(LruCache::with_capacity(2048)),
        };

        for limit in qos_rules {
            r.qos_state.borrow_mut().push(Bucket::new(limit));
        }

        let mut v4_hashmap: HashMap<(Ipv4Addr, u32), Vec<usize>> = HashMap::new();
        let mut v6_hashmap: HashMap<(Ipv6Addr, u32), Vec<usize>> = HashMap::new();

        for (index, rule) in rules.into_iter().enumerate() {
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
                    v4_hashmap
                        .entry((subnet.mask(mask), mask))
                        .or_default()
                        .push(index);
                }
                (IpAddr::V6(subnet), mask) => {
                    v6_hashmap
                        .entry((subnet.mask(mask), mask))
                        .or_default()
                        .push(index);
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

    pub fn is_acceptable(
        &self,
        device: Device,
        protocol: Proto,
        addr: SocketAddr,
        len: usize,
        exe: &str,
    ) -> (Option<usize>, bool) {
        let mut hasher = DefaultHasher::new();
        (device, protocol, addr, exe).hash(&mut hasher);
        let lru_index = hasher.finish();

        let mut cache = self.cache.borrow_mut();
        let (rule_id, target) = cache.get(&lru_index).cloned().unwrap_or_else(|| {
            let result = self.match_target(device, protocol, addr, exe);
            cache.insert(lru_index, result);
            result
        });

        let accept = match target {
            RuleTarget::Accept => true,
            RuleTarget::Drop => false,
            RuleTarget::Qos(qos_id) => self.qos_state.borrow_mut()[qos_id].stuff(len),
        };
        (rule_id, accept)
    }

    fn match_target(
        &self,
        device: Device,
        protocol: Proto,
        addr: SocketAddr,
        exe: &str,
    ) -> (Option<usize>, RuleTarget) {
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
            .map(|(id, t)| (Some(id), t))
            .unwrap_or((None, self.default_target))
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn rules_indexing() {
        let raw_rules = vec![
            Rule {
                device: Some(Device::Input),
                proto: None,
                exe: None,
                port: None,
                subnet: ([1, 1, 1, 1].into(), 32),
                target: RuleTarget::Accept,
            },
            Rule {
                device: Some(Device::Input),
                proto: Some(Proto::Tcp),
                exe: None,
                port: None,
                subnet: ([1, 1, 1, 1].into(), 32),
                target: RuleTarget::Accept,
            },
            Rule {
                device: Some(Device::Input),
                proto: Some(Proto::Tcp),
                exe: None,
                port: None,
                subnet: ([2, 2, 2, 2].into(), 30),
                target: RuleTarget::Accept,
            },
            Rule {
                device: Some(Device::Input),
                proto: None,
                exe: Some("".into()),
                port: Some(RangeInclusive::new(10, 200)),
                subnet: ([2, 2, 2, 2].into(), 32),
                target: RuleTarget::Accept,
            },
            Rule {
                device: Some(Device::Input),
                proto: None,
                exe: Some("".into()),
                port: Some(RangeInclusive::new(100, 100)),
                subnet: ([0, 0, 0, 0].into(), 0),
                target: RuleTarget::Accept,
            },
        ];

        let mut device = HashMap::new();
        device.insert(Device::Input, vec![0, 1, 2, 3, 4]);
        let mut proto = HashMap::new();
        proto.insert(Proto::Tcp, vec![1, 2]);
        let mut exe = HashMap::new();
        exe.insert("".into(), vec![3, 4]);
        let mut port = HashMap::new();
        for p in 10..=200 {
            port.insert(p, vec![3]);
        }
        port.get_mut(&100).unwrap().push(4);
        let mut v4_hashmap = HashMap::new();
        v4_hashmap.insert(([1, 1, 1, 1], 32), vec![0, 1]);
        v4_hashmap.insert(([2, 2, 2, 2], 30), vec![2]);
        v4_hashmap.insert(([2, 2, 2, 2], 32), vec![3]);
        v4_hashmap.insert(([0, 0, 0, 0], 0), vec![4]);

        let r: Rules = Rules::new(RuleTarget::Drop, raw_rules.clone(), vec![]);
        assert_eq!(r.device, device);
        assert_eq!(r.any_device, vec![]);
        assert_eq!(r.proto, proto);
        assert_eq!(r.any_proto, vec![0, 3, 4]);
        assert_eq!(r.exe, exe);
        assert_eq!(r.any_exe, vec![0, 1, 2]);
        assert_eq!(r.port, port);
        assert_eq!(r.any_port, vec![0, 1, 2]);
        assert_eq!(r.raw, raw_rules);
        assert_eq!(r.default_target, RuleTarget::Drop);

        assert_eq!(
            r.is_acceptable(Device::Input, Proto::Tcp, ([2, 2, 2, 2], 100).into(), 0, "",),
            (Some(3), true)
        );
    }
}