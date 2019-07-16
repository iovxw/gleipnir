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

fn iptables(v4: bool, output: bool, cmd: &str, queue_num: u16) -> Command {
    let mut c = Command::new(if v4 { "iptables" } else { "ip6tables" });
    c.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .arg(format!("-{}", cmd))
        .arg(if output { "OUTPUT" } else { "INPUT" })
        .arg("!")
        .arg(if output { "-o" } else { "-i" })
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
    fn insert_if_not_exists(v4: bool, output: bool, num: u16) {
        let rule_existed = iptables(v4, output, "C", num).status().unwrap().success();
        if !rule_existed {
            iptables(v4, output, "I", num).status().unwrap().success();
        }
    }

    insert_if_not_exists(false, false, num);
    insert_if_not_exists(true, false, num);
    insert_if_not_exists(false, true, num);
    insert_if_not_exists(true, true, num);
}

fn iptables_remove_nfqueue(num: u16) {
    iptables(false, false, "D", num).status().unwrap().success();
    iptables(true, false, "D", num).status().unwrap().success();
    iptables(false, true, "D", num).status().unwrap().success();
    iptables(true, true, "D", num).status().unwrap().success();
}
