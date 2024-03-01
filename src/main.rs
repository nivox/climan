use std::{collections::HashMap, env, fs::File, process::ExitCode};

mod climan;

use clap::{Parser, Subcommand};
use climan::request::{Request, RequestContext, Response};
use dotenv::dotenv;
use log::{error, LevelFilter};
use schemars::schema_for;
use simplelog::{
    ColorChoice, CombinedLogger, Config, SharedLogger, TermLogger, TerminalMode, WriteLogger,
};

use crate::climan::workflow::Workflow;

fn on_request(request: &Request, context: &RequestContext) -> () {
    println!(
        "Executing request {}, with variables:\n{}",
        request.name,
        serde_json::to_string_pretty(context.variables).unwrap_or("err".to_string())
    );
}

fn on_response(_request: &Request, _context: &RequestContext, response: &Response) -> () {
    println!(
        "Response status: {}\nHeaders:\n{}\nExtracted variables:\n{}\nBody:\n{}",
        response.status_code,
        serde_json::to_string_pretty(&response.headers).unwrap_or("err".to_string()),
        serde_json::to_string_pretty(&response.extracted_variables).unwrap_or("err".to_string()),
        response.body
    );
}

/*
fn print_response(
    final_uri: String,
    status: StatusCode,
    json_value: Option<serde_json::Value>,
    body_string: String,
    request: Request,
    headers: HeaderMap,
    extracted_variables: HashMap<String, Option<String>>,
) -> anyhow::Result<()> {
    let status_color = if status.is_client_error() || status.is_server_error() {
        "red"
    } else if status.is_redirection() || status.is_informational() {
        "yellow"
    } else {
        "green"
    };

    let body_formatted = match json_value.as_ref() {
        Some(json) => serde_json::to_string_pretty(json)?,
        None => body_string,
    };

    let mut headers_string = String::new();
    for header in headers {
        match header {
            (name, value) => {
                let name_string = name
                    .map(|name| name.as_str().to_string())
                    .unwrap_or("default".into());
                let with_value = format!("{} : {}\n", name_string, value.to_str()?.to_string());
                headers_string.push_str(&with_value)
            }
        }
    }

    let mut variables_string = String::new();

    if extracted_variables.is_empty() {
        variables_string.push_str("")
    } else {
        for variable in extracted_variables {
            match variable {
                (name, value) => variables_string.push_str(&format!(
                    "\n  {} : {}",
                    name,
                    value.unwrap_or("".into())
                )),
            }
        }
    }

    info!(
        "---\nExecuted Request <blue>{}</>\n<white>{} {}</>\n<{}>{}</>\n---\n<bright-magenta>{}</>\n---\n<magenta>{}</>\n---\nExtracted Values:\n<cyan>[{}\n]</>",
        request.name, request.method, final_uri, status_color, status, headers_string, body_formatted, variables_string
    );
    Ok(())
}*/

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// set this to log the output into the .climan.log file in the current folder
    #[arg(short, long)]
    log: Option<bool>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Executes a workflow
    Workflow {
        /// Path to the workflow file
        path: String,

        /// Initial variables to be used in the workflow in the format name=value
        #[arg(short, long)]
        variables: Option<Vec<String>>,

        /// Include environment variables as initial variables
        #[arg(short, long)]
        env: bool,
    },
    
    /// Executes a single request
    Request {
        /// Path to the request file
        path: String,

        /// Initial variables to be used in the request in the format name=value
        #[arg(short, long)]
        variables: Option<Vec<String>>,

        /// Include environment variables as initial variables
        #[arg(short, long)]
        env: bool,
    },

    /// Prints the schema for the workflow
    Schema,
}

fn parse_variables(variables: Vec<String>) -> HashMap<String, Option<String>> {
    variables
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

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    dotenv().ok();

    let cli = Cli::parse();

    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![TermLogger::new(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )];

    if cli.log.unwrap_or(false) {
        let writer_logger = WriteLogger::new(
            LevelFilter::Info,
            Config::default(),
            File::create(".climan.log").unwrap(),
        );
        loggers.append(&mut vec![writer_logger])
    };

    CombinedLogger::init(loggers).expect("unable to setup logging");

    match cli.command {
        Command::Workflow {
            path,
            variables,
            env,
        } => {
            let content = std::fs::read_to_string(path)?;
            let workflow: Workflow = serde_yaml::from_str(&content)?;

            let mut all_vars = variables.map_or(HashMap::new(), parse_variables);
            if env {
                for (key, value) in env::vars() {
                    all_vars.insert(key, Some(value));
                }
            }

            let client = reqwest::Client::new();
            let result = workflow
                .execute(&client, all_vars, on_request, on_response)
                .await;

            if result.is_err() {
                let error = result.unwrap_err();
                log::error!("could not execute workflow, error: {:?}", error);
                Ok(ExitCode::FAILURE)
            } else {
                Ok(ExitCode::SUCCESS)
            }
        },
        Command::Request {
            path,
            variables,
            env,
        } => {
            let content = std::fs::read_to_string(path)?;
            let request: Request = serde_yaml::from_str(&content)?;

            let mut all_vars = variables.map_or(HashMap::new(), parse_variables);
            if env {
                for (key, value) in env::vars() {
                    all_vars.insert(key, Some(value));
                }
            }

            let client = reqwest::Client::new();
            let context = RequestContext::new(&all_vars); 
            on_request(&request, &context);
            let result = request
                .execute(&client, &context)
                .await;

            if result.is_err() {
                let error = result.unwrap_err();
                log::error!("could not execute workflow, error: {:?}", error);
                Ok(ExitCode::FAILURE)
            } else {
                on_response(&request, &context, &result.unwrap());
                Ok(ExitCode::SUCCESS)
            }
        },
        
        Command::Schema => {
            let schema = schema_for!(Workflow);
            println!("{}", serde_json::to_string_pretty(&schema).unwrap());
            Ok(ExitCode::SUCCESS)
        }
    }
}
