use std::{collections::HashMap, fs::File, process::ExitCode};

mod climan;

use clap::Parser;
use climan::{execute_spec, ApiSpec};
use log::{error, LevelFilter};
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode, WriteLogger};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    spec: String,

    #[arg(short, long)]
    variables: Option<Vec<String>>,
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
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Info,
            Config::default(),
            File::create(".climan.log").unwrap(),
        ),
    ])
    .expect("unable to setup logging");

    let args = Args::parse();

    let content = std::fs::read_to_string(&args.spec)?;
    let api_spec: ApiSpec = serde_yaml::from_str(&content)?;
    let result = execute_spec(api_spec, args.parse_variables()).await?;

    if result.last_error.is_some() {
        Ok(ExitCode::FAILURE)
    } else {
        Ok(ExitCode::SUCCESS)
    }
}
