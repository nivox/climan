use std::{collections::HashMap, env, fs::File, process::ExitCode};

mod climan;

use clap::Parser;
use climan::{execute_spec, ApiSpec};
use dotenv::dotenv;
use log::{error, LevelFilter};
use simplelog::{
    ColorChoice, CombinedLogger, Config, SharedLogger, TermLogger, TerminalMode, WriteLogger,
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    // if no #[arg] directive is specified the argument is treated as positional
    // so we expect the spec path to be the first argument
    /// the path to a YAML specification
    spec: String,

    /// additional variables to set in the format: FOO=BAR,
    /// note that you can also set variables with .env file,
    /// the current environmant variables are already available without this option
    #[arg(short, long)]
    variables: Option<Vec<String>>,

    /// set this to log the output into the .climan.log file in the current folder
    #[arg(short, long)]
    log: Option<bool>,
}

impl Args {
    fn parse_variables(&self) -> HashMap<String, Option<String>> {
        self.variables
            .clone()
            .unwrap_or(vec![])
            .into_iter()
            .flat_map(|variable_spec| {
                let split: Vec<&str> = variable_spec.split("=").into_iter().collect();
                match (split.first(), split.iter().nth(1)) {
                    (Some(name), Some(value)) => vec![(name.to_string(), Some(value.to_string()))],
                    (name, value) => {
                        error!("invalid variable spec: {:?}, {:?}", name, value);
                        vec![]
                    }
                }
            })
            .collect()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    dotenv().ok();

    let args = Args::parse();

    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![TermLogger::new(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )];

    if args.log.unwrap_or(false) {
        let writer_logger = WriteLogger::new(
            LevelFilter::Info,
            Config::default(),
            File::create(".climan.log").unwrap(),
        );
        loggers.append(&mut vec![writer_logger])
    };

    CombinedLogger::init(loggers).expect("unable to setup logging");

    let content = std::fs::read_to_string(&args.spec)?;
    let api_spec: ApiSpec = serde_yaml::from_str(&content)?;

    let mut all_vars = args.parse_variables();
    for (key, value) in env::vars() {
        all_vars.insert(key, Some(value));
    }

    let result = execute_spec(api_spec, all_vars).await?;

    if result.last_error.is_some() {
        log::error!("could not execute API spec, error: {:?}", result.last_error);
        Ok(ExitCode::FAILURE)
    } else {
        Ok(ExitCode::SUCCESS)
    }
}
