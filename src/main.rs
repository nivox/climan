use clap::{Parser, Subcommand};
use log::{error, LevelFilter};
use schemars::schema_for;

use std::borrow::Borrow;
use std::path::PathBuf;
use std::{collections::HashMap, env, fs::File, process::ExitCode};
use termimad::minimad::TextTemplate;
use termimad::MadSkin;

mod climan;
use climan::request::{Request, RequestContext, Response};
use climan::workflow::Workflow;

fn print_header_table<'v, T: IntoIterator<Item = (&'v str, &'v str)>>(
    skin: &MadSkin,
    header_map: T,
) {
    let template = TextTemplate::from(
        r#"
    | :-: | :-: |
    | **Header** | **Value** |
    | :- | :- |
    ${rows
    | *${name}* | ${value} |
    }
    | - | - |
    "#,
    );

    let mut expander = template.expander();
    for (name, value) in header_map {
        expander.sub("rows").set("name", name).set("value", value);
    }

    skin.print_expander(expander);
}

fn print_variable_table(skin: &MadSkin, variables: &HashMap<String, Option<String>>) {
    let template = TextTemplate::from(
        r#"
    | :-: | :-: |
    | **Variable** | **Value** |
    | :- | :- |
    ${rows
    | *${name}* | ${value} |
    }
    | - | - |
    "#,
    );

    let mut expander = template.expander();
    for (name, value) in variables {
        let value = value.as_ref().map(|v| v.as_str()).unwrap_or("");
        expander.sub("rows").set("name", name).set("value", value);
    }

    skin.print_expander(expander);
}

fn on_request(skin: MadSkin, request: &Request, context: &RequestContext) {
    let step_template = TextTemplate::from("# 📗 Executing step: ${name}");
    let mut step_expander = step_template.expander();
    step_expander.set("name", &request.name);

    skin.print_expander(step_expander);

    skin.print_text("* **Variables:**");
    print_variable_table(&skin, context.variables);
    println!();

    let template = TextTemplate::from(
        r#"
## 📤 Request properties
* **Method**: ${method}
* **URL**: ${url}"#,
    );
    let mut expander = template.expander();
    let method_name = context.method.to_string();
    expander
        .set("name", &request.name)
        .set("method", &method_name)
        .set("url", &context.uri);
    skin.print_expander(expander);

    skin.print_text("* **Headers:**");
    print_header_table(
        &skin,
        context
            .headers
            .borrow()
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str())),
    );

    skin.print_text("* **Body:**");
    let body_template = TextTemplate::from("```\n${body}\n```");
    let mut body_expander = body_template.expander();
    let body_content = context.body.as_deref().unwrap_or("");
    body_expander.set_lines("body", body_content);
    skin.print_expander(body_expander);
    println!();
}

fn on_response(skin: MadSkin, _request: &Request, _context: &RequestContext, response: &Response) {
    let template = TextTemplate::from(
        r#"
## 📥 Response properties
* **Status**: ${status_color} ${status_code}
* **Time to Headers:** ${time_to_headers}ms
* **Time total:** ${time_total}ms"#,
    );
    let mut expander = template.expander();

    let status_color = match response.status_code {
        200..=299 => "🟢",
        300..=399 => "🟠",
        400..=499 => "🔴",
        500..=599 => "🔥",
        _ => "",
    };
    let status_code = response.status_code.to_string();
    let time_to_headers = response.time_to_headers.as_millis().to_string();
    let time_total = response.time_total.as_millis().to_string();

    expander
        .set("status_color", status_color)
        .set("status_code", &status_code)
        .set("time_to_headers", &time_to_headers)
        .set("time_total", &time_total);

    skin.print_expander(expander);

    skin.print_text("* **Headers:**");
    print_header_table(
        &skin,
        response
            .headers
            .borrow()
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str())),
    );

    skin.print_text("* **Extracted variables:**");
    print_variable_table(&skin, &response.extracted_variables);

    skin.print_text("* **Body:**");
    let body_template = TextTemplate::from("```\n${body}\n```");
    let mut body_expander = body_template.expander();
    body_expander.set_lines("body", &response.body);
    skin.print_expander(body_expander);
    println!();
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// set this to log the output into the .climan.log file in the current folder
    #[arg(short, long)]
    log_file: Option<bool>,

    /// set the log verbosity level: 0=off, 1=error, 2=warn, 3=info, 4=debug, 5=trace (default: 2)
    log_level: Option<u8>,
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

        /// yaml files with additional variables
        #[arg(short, long)]
        files: Option<Vec<PathBuf>>,

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
            let split: Vec<&str> = variable_spec.split('=').collect();
            match (split.first(), split.get(1)) {
                (Some(name), Some(value)) => vec![(name.to_string(), Some(value.to_string()))],
                (name, value) => {
                    error!("invalid variable spec: {:?}, {:?}", name, value);
                    vec![]
                }
            }
        })
        .collect()
}

fn init_variables(variables: Option<Vec<String>>, env: bool) -> HashMap<String, Option<String>> {
    let mut all_vars = variables.map_or(HashMap::new(), parse_variables);
    if env {
        for (key, value) in env::vars() {
            all_vars.insert(key, Some(value));
        }
    }
    all_vars
}

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();

    let log_level = match cli.log_level.unwrap_or(2) {
        0 => LevelFilter::Off,
        1 => LevelFilter::Error,
        2 => LevelFilter::Warn,
        3 => LevelFilter::Info,
        4 => LevelFilter::Debug,
        5 => LevelFilter::Trace,
        _ => LevelFilter::Warn,
    };

    let mut loggers: Vec<Box<dyn simplelog::SharedLogger>> = vec![simplelog::TermLogger::new(
        log_level,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )];

    if cli.log_file.unwrap_or(false) {
        loggers.push(simplelog::WriteLogger::new(
            log_level,
            simplelog::Config::default(),
            File::create(".climan.log").unwrap(),
        ));
    };

    simplelog::CombinedLogger::init(loggers).expect("unable to setup logging");

    let skin: MadSkin = serde_yaml::from_str(include_str!("../assets/skin.yaml"))?;
    let skinned_on_request =
        |request: &Request, context: &RequestContext| on_request(skin.clone(), request, context);
    let skinned_on_response = |request: &Request, context: &RequestContext, response: &Response| {
        on_response(skin.clone(), request, context, response)
    };

    match cli.command {
        Command::Workflow {
            path,
            variables,
            files,
            env,
        } => {
            let content = std::fs::read_to_string(path)?;
            let workflow: Workflow = serde_yaml::from_str(&content)?;

            let all_vars = init_variables(variables, env);
            let client = reqwest::Client::new();

            let workflow_template = TextTemplate::from("# 🚀 Executing workflow: ${name}");
            let mut workflow_expander = workflow_template.expander();
            workflow_expander.set("name", &workflow.name);

            skin.print_expander(workflow_expander);
            let result = workflow
                .execute(
                    &client,
                    all_vars,
                    files,
                    &skinned_on_request,
                    &skinned_on_response,
                )
                .await;

            if result.is_err() {
                log::error!(
                    "could not execute workflow, error: {:?}",
                    result.unwrap_err()
                );
                Ok(ExitCode::FAILURE)
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::Request {
            path,
            variables,
            env,
        } => {
            let content = std::fs::read_to_string(path)?;
            let request: Request = serde_yaml::from_str(&content)?;

            let all_vars = init_variables(variables, env);

            let client = reqwest::Client::new();
            let result = request
                .execute(
                    &client,
                    &all_vars,
                    &skinned_on_request,
                    &skinned_on_response,
                )
                .await;

            if result.is_err() {
                log::error!(
                    "could not execute request, error: {:?}",
                    result.unwrap_err()
                );
                Ok(ExitCode::FAILURE)
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }

        Command::Schema => {
            let schema = schema_for!(Workflow);
            println!("{}", serde_json::to_string_pretty(&schema).unwrap());
            Ok(ExitCode::SUCCESS)
        }
    }
}
