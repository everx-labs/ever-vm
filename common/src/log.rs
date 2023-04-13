#[allow(dead_code)]
fn init_log(config: &str) {
    if !log::log_enabled!(log::Level::Error) {
        log4rs::init_file(config, Default::default())
            .unwrap_or_else(|_| panic!("Cannot read logging configuration from {}", config));
    }
}

#[allow(dead_code)]
fn init_log_without_config(log_level: log::LevelFilter, output_file: Option<&str>) {
    let encoder_boxed = Box::new(log4rs::encode::pattern::PatternEncoder::new("{m}"));
    let config = if let Some(file) = output_file {
        let file = log4rs::append::file::FileAppender::builder()
            .encoder(encoder_boxed)
            .build(file)
            .unwrap();
        log4rs::config::Config::builder()
            .appender(log4rs::config::Appender::builder().build("file", Box::new(file)))
            .build(log4rs::config::Root::builder().appender("file").build(log_level))
            .unwrap()
    } else {
        let console = log4rs::append::console::ConsoleAppender::builder()
            .encoder(encoder_boxed)
            .build();
        log4rs::config::Config::builder()
            .appender(log4rs::config::Appender::builder().build("console", Box::new(console)))
            .build(log4rs::config::Root::builder().appender("console").build(log_level))
            .unwrap()
    };
    log4rs::init_config(config).ok();
}
