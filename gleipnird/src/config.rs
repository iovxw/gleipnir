use std::fs::{create_dir_all, File};
use std::path::PathBuf;

use failure;
use gleipnir_interface::{RuleTarget, Rules};
use lazy_static::lazy_static;
use serde_json;

lazy_static! {
    static ref CONFIG_DIR: PathBuf = {
        let dir = option_env!("GLEIPNIRD_CONFIG_DIR")
            .unwrap_or(concat!("/etc/", env!("CARGO_PKG_NAME")))
            .into();
        create_dir_all(&dir).expect("Failed to create config directory");
        dir
    };
}

pub fn save_rules(rules: &Rules) {
    let r: Result<(), failure::Error> = try {
        let f = File::create(CONFIG_DIR.join("rules.json"))?;
        serde_json::to_writer(f, &rules)?;
    };
    if let Err(e) = r {
        dbg!(e);
    }
}

pub fn load_rules() -> Result<Rules, failure::Error> {
    let path = CONFIG_DIR.join("rules.json");
    if !path.exists() {
        return Ok(Rules {
            default_target: RuleTarget::Accept,
            rules: Default::default(),
            rate_rules: Default::default(),
        });
    }
    let f = File::open(path)?;
    Ok(serde_json::from_reader(f)?)
}
