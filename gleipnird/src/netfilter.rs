use std::process::{exit, Command, Stdio};

use ctrlc;

pub fn register_nfqueue(num: u16) {
    if nft_exists() {

    } else {
        iptables_insert_nfqueue(num);
        ctrlc::set_handler(move || {
            iptables_remove_nfqueue(num);
            exit(0);
        })
        .expect("Error setting Ctrl-C handler");
    }
}

// TODO: or just iptables-nft?
fn nft_exists() -> bool {
    false
}

fn iptables(exe: &str, cmd: &str, queue_num: u16) -> Command {
    let mut c = Command::new(exe);
    c.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .arg(format!("-{}", cmd))
        .arg("OUTPUT")
        .arg("!")
        .arg("-o")
        .arg("lo")
        .arg("-t")
        .arg("mangle")
        .arg("-j")
        .arg("NFQUEUE")
        .arg("--queue-num")
        .arg(queue_num.to_string())
        .arg("--queue-bypass");
    c
}

fn iptables_insert_nfqueue(num: u16) {
    let rule_existed = iptables("iptables", "C", num).status().unwrap().success();
    if !rule_existed {
        iptables("iptables", "I", num).status().unwrap().success();
    }
    let rule_existed = iptables("ip6tables", "C", num).status().unwrap().success();
    if !rule_existed {
        iptables("ip6tables", "I", num).status().unwrap().success();
    }
}

fn iptables_remove_nfqueue(num: u16) {
    iptables("iptables", "D", num).status().unwrap().success();
    iptables("ip6tables", "D", num).status().unwrap().success();
}
