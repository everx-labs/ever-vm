fn init_log(config: &str) {
    if !log::log_enabled!(log::Level::Error) {
        log4rs::init_file(config, Default::default())
            .unwrap_or_else(|_| panic!("Cannot read logging configuration from {}", config));
    }
}

