use std::collections::HashMap;

use log::debug;
use reqwest::Client;
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

        let mut context: WorkflowContext = WorkflowContext::new(variables);
        let mut responses: Vec<Response> = Vec::new();

        for request in &self.requests {
            debug!("executing request: {:?}", request);

            let response = request.execute(client, &context.variables, request_action, response_action).await?;

            context.update(response.extracted_variables.clone());
            responses.push(response);
        }

        Ok(WorkflowResult {
            responses,
            final_variables: context.variables,
        })
    }
}
