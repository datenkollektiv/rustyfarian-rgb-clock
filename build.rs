use std::collections::HashMap;
use std::fs;

/// Required environment variables for the application
const REQUIRED_ENV_VARS: &[(&str, &str)] = &[
    ("WIFI_SSID", "WiFi network name"),
    ("WIFI_PASS", "WiFi password"),
    ("MQTT_HOST", "MQTT broker hostname or IP"),
    ("MQTT_PORT", "MQTT broker port (e.g., 1883)"),
    ("MQTT_CLIENT_ID", "Unique MQTT client identifier"),
];

fn main() {
    // Only re-run if .env changes
    println!("cargo:rerun-if-changed=.env");

    let mut env_vars: HashMap<String, String> = HashMap::new();

    // Read .env file if it exists
    if let Ok(env_content) = fs::read_to_string(".env") {
        for line in env_content.lines() {
            let line = line.trim();
            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Parse KEY=VALUE
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim().to_string();
                let value = value.trim().to_string();
                println!("cargo:rustc-env={}={}", key, value);
                env_vars.insert(key, value);
            }
        }
    } else {
        println!("cargo:warning===========================================");
        println!("cargo:warning=No .env file found!");
        println!("cargo:warning=Copy .env.example to .env and fill in values:");
        println!("cargo:warning=  cp .env.example .env");
        println!("cargo:warning===========================================");
    }

    // Validate required environment variables
    let missing: Vec<_> = REQUIRED_ENV_VARS
        .iter()
        .filter(|(key, _)| {
            let value = env_vars.get(*key);
            value.is_none() || value.is_some_and(|v| v.is_empty())
        })
        .collect();

    if !missing.is_empty() {
        println!("cargo:warning===========================================");
        println!("cargo:warning=Missing required environment variables:");
        for (key, description) in &missing {
            println!("cargo:warning=  {} - {}", key, description);
        }
        println!("cargo:warning=");
        println!("cargo:warning=Add these to your .env file.");
        println!("cargo:warning=See .env.example for reference.");
        println!("cargo:warning===========================================");
    }

    embuild::espidf::sysenv::output();
}
