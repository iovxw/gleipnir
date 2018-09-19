use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, prelude::*};
use std::path::PathBuf;

type Pid = usize;
type Inode = u32;

const PROC: &str = "/proc/";

thread_local! {
    static PROC_CACHE: RefCell<ProcCache> = Default::default();
    static INODE_INDEX: RefCell<HashMap<Inode, Pid>> = Default::default();
    static PROC_INDEX: RefCell<HashMap<Pid, Process>> = Default::default();
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
        INODE_INDEX.with(|inode_index| {
            let inodes = inode_index.borrow();
            inodes.get(&inode).map(|pid| {
                PROC_INDEX.with(|proc_index| {
                    let procs = proc_index.borrow();
                    let proc = procs.get(pid).expect("broken cache");
                    proc.clone()
                })
            })
        })
    }
    if inode == 0 {
        return None;
    }
    get(inode)
        .or_else(|| {
            add_new_proc_to_cache();
            get(inode)
        }).or_else(|| {
            refresh_old_proc_in_cache();
            get(inode)
        })
}

fn add_new_proc_to_cache() {
    PROC_CACHE.with(|proc_cache| {
        INODE_INDEX.with(|inode_index| {
            PROC_INDEX.with(|proc_index| {
                let mut cache = proc_cache.borrow_mut();
                let mut inodes = inode_index.borrow_mut();
                let mut procs = proc_index.borrow_mut();
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
                        let proc = match parse_proc_pid(entry.path(), pid) {
                            Ok(r) => r,
                            Err(_) => continue,
                        };
                        for &inode in &proc.inodes {
                            inodes.insert(inode, pid);
                        }
                        procs.insert(pid, proc);
                        new.insert(pid);
                    }
                }
                for pid in &*garbage {
                    let proc = procs.remove(pid).expect("");
                    for inode in &proc.inodes {
                        inodes.remove(inode);
                    }
                }
            })
        })
    })
}

fn refresh_old_proc_in_cache() {
    PROC_CACHE.with(|proc_cache| {
        INODE_INDEX.with(|inode_index| {
            PROC_INDEX.with(|proc_index| {
                let mut cache = proc_cache.borrow_mut();
                let mut inodes = inode_index.borrow_mut();
                let mut procs = proc_index.borrow_mut();
                let ProcCache { new, old, .. } = &mut *cache;
                for pid in old.drain() {
                    let path: PathBuf = format!("{}{}", PROC, pid).into();
                    if let Ok(proc) = parse_proc_pid(path, pid) {
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
            })
        })
    })
}

// http://manpages.ubuntu.com/manpages/bionic/en/man5/proc.5.html
fn parse_proc_pid(mut path: PathBuf, pid: usize) -> Result<Process, io::Error> {
    path.push("fd");
    let mut inodes = Vec::new();
    for file in fs::read_dir(&path)? {
        let path = fs::read_link(file?.path())?;
        let path = path.to_str().expect("symlink not a vaild UTF-8");
        if path.starts_with("socket:[") && path.ends_with("]") {
            let inode = path[8..path.len() - 1]
                .parse::<Inode>()
                .expect("inode not a number");
            inodes.push(inode);
        }
    }
    path.pop();
    path.push("exe");
    let exe = fs::read_link(&path)
        .unwrap_or_default()
        .to_str()
        .expect("symlink not a vaild UTF-8")
        .to_owned();
    path.pop();
    path.push("stat");
    let mut stat = File::open(path)?;
    let mut buf = [0u8; 512];
    let n = stat.read(&mut buf)?;
    let stat = std::str::from_utf8(&buf[..n]).expect("stat not a vaild UTF-8");
    // TODO: expect message
    let mut iter = stat.rsplit(')').next().expect("").split(' ').skip(2);
    let ppid = iter.next().expect("").parse().expect("");
    let pgrp = iter.next().expect("").parse().expect("");
    Ok(Process {
        pid,
        ppid,
        pgrp,
        exe,
        inodes,
    })
}
