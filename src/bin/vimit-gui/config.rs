use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::ng;

#[derive(Debug, Clone, Default)]
pub(crate) struct GuiAccount {
    pub(crate) api_key_env: Option<String>,
    pub(crate) api_base: Option<String>,
}

pub(crate) fn load_gui_accounts() -> (Vec<String>, Vec<GuiAccount>) {
    if let Ok(config) = ng::cli::accounts::AccountsConfig::load() {
        let names = config.list_names();
        let configs: Vec<GuiAccount> = names
            .iter()
            .filter_map(|name| config.resolve(name).ok())
            .map(|resolved| GuiAccount {
                api_key_env: resolved.api_key_env.clone(),
                api_base: resolved.api_base.clone(),
            })
            .collect();
        (names, configs)
    } else {
        (vec![], vec![])
    }
}

pub(crate) fn gui_load_dotenv() -> Result<HashMap<String, String>, String> {
    ng::load_dotenv_custom(None).or_else(|error| {
        eprintln!("vimit-gui: {error}");
        Ok(HashMap::new())
    })
}

pub(crate) fn runtime_config(
    dotenv: &HashMap<String, String>,
    account: &Arc<Mutex<Option<GuiAccount>>>,
    auto_failover: bool,
) -> ng::RuntimeConfig {
    let account = account.lock().unwrap();
    let (api_key_env, api_base_override) = match account.as_ref() {
        Some(account) => (
            account.api_key_env.as_deref().unwrap_or("VIBEMODE_API_KEY"),
            account.api_base.clone(),
        ),
        None => ("VIBEMODE_API_KEY", None),
    };
    ng::RuntimeConfig {
        api_base: api_base_override
            .or_else(|| ng::config_value("VIBEMODE_API_BASE", dotenv))
            .unwrap_or_else(|| ng::DEFAULT_API_BASE.to_string()),
        api_key: ng::config_value(api_key_env, dotenv).unwrap_or_default(),
        abtop_bin: ng::config_value("ABTOP_BIN", dotenv).unwrap_or_default(),
        auto_failover,
    }
}
