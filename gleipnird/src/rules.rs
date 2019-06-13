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

use gleipnir_interface::{Address, Device, Proto, Rule, RuleTarget, Rules};

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

pub struct IndexedRules {
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
    rate_state: RefCell<Vec<Bucket>>,
    cache: RefCell<LruCache<u64, (Option<usize>, RuleTarget)>>,
}

impl IndexedRules {
    pub fn new(default_target: RuleTarget, rules: Vec<Rule>, rate_rules: Vec<usize>) -> Self {
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
            rate_state: Default::default(),
            cache: RefCell::new(LruCache::with_capacity(2048)),
        };

        for limit in rate_rules {
            r.rate_state.borrow_mut().push(Bucket::new(limit));
        }

        let mut v4_hashmap: HashMap<(Ipv4Addr, u8), Vec<usize>> = HashMap::new();
        let mut v6_hashmap: HashMap<(Ipv6Addr, u8), Vec<usize>> = HashMap::new();

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
            r.v4_table.insert(ip, masklen.into(), index);
        }
        for ((ip, masklen), index) in v6_hashmap {
            r.v6_table.insert(ip, masklen.into(), index);
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
            RuleTarget::RateLimit(rate_id) => self.rate_state.borrow_mut()[rate_id].stuff(len),
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

impl From<Rules> for IndexedRules {
    fn from(r: Rules) -> Self {
        Self::new(
            r.default_target,
            r.rules,
            r.rate_rules.into_iter().map(|r| r.limit).collect(),
        )
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

        let r = IndexedRules::new(RuleTarget::Drop, raw_rules.clone(), vec![]);
        assert_eq!(r.device, device);
        assert_eq!(r.any_device, Vec::<usize>::new());
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
