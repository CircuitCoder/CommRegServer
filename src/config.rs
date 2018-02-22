use serde_yaml;
use std::fs::File;

const CONFIG_PATH: &str = "./config.yml";

#[derive(Serialize, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub web: ServerConfig,
    pub ws: ServerConfig,

    #[serde(skip_serializing)] // Avoids accidental leak
    pub secret: String,
    pub proxied: Option<String>,
}

impl Config {
    pub fn load() -> Config {
        // TODO: arbitary config file
        let f = File::open(CONFIG_PATH);
        serde_yaml::from_reader(f.unwrap()).unwrap()
    }
}
