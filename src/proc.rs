use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::prelude::*;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use lazy_static::*;

type Pid = usize;
type Inode = u32;

const PROC: &str = "/proc/";

lazy_static! {
    static ref PROC_CACHE: Mutex<ProcCache> = Default::default();
    static ref INODE_INDEX: RwLock<HashMap<Inode, Pid>> = Default::default();
    static ref PROC_INDEX: RwLock<HashMap<Pid, Process>> = Default::default();
}

#[derive(Debug, Clone)]
pub struct Process {
    pub pid: usize,
    /// The PID of the parent of this process.
    pub ppid: usize,
    /// The process group ID of the process.
    pub pgrp: usize,
    pub exe: String,
    pub inodes: Vec<Inode>,
}

#[derive(Default)]
struct ProcCache {
    new: HashSet<Pid>,
    old: HashSet<Pid>,
    garbage: HashSet<Pid>,
}

pub fn get_proc_by_inode(inode: Inode) -> Option<Process> {
    fn get(inode: Inode) -> Option<Process> {
        let inodes = INODE_INDEX.read().unwrap();
        inodes.get(&inode).map(|pid| {
            let procs = PROC_INDEX.read().unwrap();
            let proc = procs.get(pid).expect("");
            proc.clone()
        })
    }
    get(inode).or_else(|| {
        add_new_proc_to_cache();
        get(inode)
    }).or_else(|| {
        refresh_old_proc_in_cache();
        get(inode)
    })
}

fn add_new_proc_to_cache() {
    let mut cache = PROC_CACHE.lock().unwrap();
    let ProcCache { new, old, garbage } = &mut *cache;
    garbage.clear();
    garbage.extend(new.drain());
    garbage.extend(old.drain());
    for (entry, pid) in fs::read_dir(PROC)
        .expect("open /proc")
        .map(|e| e.expect("visit /proc"))
        .filter_map(|e| {
            let path = e.path();
            let file_name = path
                .file_name()
                .expect("no file_name")
                .to_str()
                .expect("file_name not a vaild UTF-8");
            match file_name.parse::<Pid>() {
                Ok(pid) => Some((e, pid)),
                _ => None,
            }
        }) {
        if garbage.remove(&pid) {
            old.insert(pid);
        } else {
            let proc = parse_proc_pid(entry.path(), pid);
            let mut inodes = INODE_INDEX.write().unwrap();
            let mut procs = PROC_INDEX.write().unwrap();
            for &inode in &proc.inodes {
                inodes.insert(inode, pid);
            }
            procs.insert(pid, proc);
            new.insert(pid);
        }
    }
    for pid in &*garbage {
        let mut inodes = INODE_INDEX.write().unwrap();
        let mut procs = PROC_INDEX.write().unwrap();
        let proc = procs.remove(pid).expect("");
        for inode in &proc.inodes {
            inodes.remove(inode);
        }
    }
}

fn refresh_old_proc_in_cache() {
    let mut cache = PROC_CACHE.lock().unwrap();
    let ProcCache { new, old, .. } = &mut *cache;
    let mut inodes = INODE_INDEX.write().unwrap();
    let mut procs = PROC_INDEX.write().unwrap();
    for pid in old.drain() {
        let path: PathBuf = format!("{}{}", PROC, pid).into();
        if path.exists() {
            let proc = parse_proc_pid(path, pid);
            for &inode in &proc.inodes {
                inodes.insert(inode, pid);
            }
            procs.insert(pid, proc);
            new.insert(pid);
        } else {
            let proc = procs.remove(&pid).expect("");
            for inode in &proc.inodes {
                inodes.remove(inode);
            }
        }
    }
}

// http://manpages.ubuntu.com/manpages/bionic/en/man5/proc.5.html
fn parse_proc_pid(mut path: PathBuf, pid: usize) -> Process {
    path.push("fd");
    let inodes = fs::read_dir(&path)
        .expect("open /proc/<pid>/fd")
        .map(|e| e.expect("visit /proc/<pid>/fd"))
        .filter_map(|e| {
            let path = fs::read_link(e.path()).expect("read /proc/<pid>/fd/<fd>");
            let path = path.to_str().expect("symlink not a vaild UTF-8");
            if path.starts_with("socket:[") && path.ends_with("]") {
                let inode = path[8..path.len() - 1]
                    .parse::<Inode>()
                    .expect("inode not a number");
                Some(inode)
            } else {
                None
            }
        })
        .collect();
    path.pop();
    path.push("exe");
    let exe = fs::read_link(&path)
        .unwrap_or_default()
        .to_str()
        .expect("symlink not a vaild UTF-8")
        .to_owned();
    path.pop();
    path.push("stat");
    let mut stat = File::open(path).expect("open /proc/<pid>/stat");
    let mut buf = [0u8; 512];
    let n = stat.read(&mut buf).expect("read /proc/<pid>/stat");
    let stat = std::str::from_utf8(&buf[..n]).expect("stat not a vaild UTF-8");
    // TODO: expect message
    let mut iter = stat.rsplit(')').next().expect("").split(' ').skip(2);
    let ppid = iter.next().expect("").parse().expect("");
    let pgrp = iter.next().expect("").parse().expect("");
    Process {
        pid,
        ppid,
        pgrp,
        exe,
        inodes,
    }
}
