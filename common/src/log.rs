fn init_log(config: &str) {
    if !log::log_enabled!(log::Level::Error) {
        log4rs::init_file(config, Default::default())
            .expect(&format!("Cannot read logging configuration from {}", config));
    }
}

