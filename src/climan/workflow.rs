use std::collections::HashMap;

use log::debug;
use reqwest::{Client, StatusCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::request::{Request, RequestContext, Response};

#[derive(Debug, Clone)]
pub struct WorkflowContext {
    variables: HashMap<String, Option<String>>,
}

impl WorkflowContext {
    pub fn new<T: IntoIterator<Item = (String, Option<String>)>>(variables: T) -> WorkflowContext {
        WorkflowContext {
            variables: HashMap::from_iter(variables),
        }
    }

    fn update<T: IntoIterator<Item = (String, Option<String>)>>(&mut self, variables: T) {
        self.variables.extend(variables);
    }
}

#[derive(Debug)]
pub struct WorkflowResult {
    pub responses: Vec<Response>,
    pub final_variables: HashMap<String, Option<String>>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct Workflow {
    pub name: String,
    pub context: Option<String>,
    requests: Vec<Request>,
}

impl Workflow {
    pub async fn execute<T: IntoIterator<Item = (String, Option<String>)>>(
        &self,
        client: &Client,
        variables: T,
        request_action: &impl Fn(&Request, &RequestContext),
        response_action: &impl Fn(&Request, &RequestContext, &Response),
    ) -> anyhow::Result<WorkflowResult> {
        debug!("executing workflow: {:?}", self.name);

        let additional_variables: HashMap<String, Option<String>> = match &self.context {
            Some(filename) => {
                debug!("loading context from file: {}", filename);
                match tokio::fs::read(filename).await {
                    Ok(context_content) => serde_yaml::from_slice(&context_content)?,
                    Err(e) => {
                        return Err(anyhow::anyhow!(
                            "failed to read context file {}: {}",
                            filename,
                            e
                        ))
                    }
                }
            }
            None => HashMap::new(),
        };

        let variables = variables.into_iter().chain(additional_variables);

        let mut context: WorkflowContext = WorkflowContext::new(variables);
        let mut responses: Vec<Response> = Vec::new();

        for request in &self.requests {
            debug!("executing request: {:?}", request);

            let response = request
                .execute(client, &context.variables, request_action, response_action)
                .await?;

            if !StatusCode::from_u16(response.status_code)?.is_success() {
                return Err(anyhow::anyhow!("request failed: {:?}", response));
            }

            context.update(response.extracted_variables.clone());
            responses.push(response);
        }

        Ok(WorkflowResult {
            responses,
            final_variables: context.variables,
        })
    }
}
