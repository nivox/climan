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

    fn as_request_context(&self) -> RequestContext {
        RequestContext::new(&self.variables)
    }

    fn update<T: IntoIterator<Item = (String, Option<String>)>>(&mut self, variables: T) -> () {
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
    name: String,
    requests: Vec<Request>,
}

impl Workflow {
    pub async fn execute<T: IntoIterator<Item = (String, Option<String>)>>(
        &self,
        client: &Client,
        variables: T,
        request_action: fn(&Request, &RequestContext) -> (),
        response_action: fn(&Request, &RequestContext, &Response) -> (),
    ) -> anyhow::Result<WorkflowResult> {
        debug!("executing workflow: {:?}", self.name);

        let mut context: WorkflowContext = WorkflowContext::new(variables);
        let mut responses: Vec<Response> = Vec::new();

        for request in &self.requests {
            debug!("executing request: {:?}", request);
            let request_context = context.as_request_context();

            request_action(request, &request_context);

            let response = request.execute(client, &request_context).await?;

            response_action(request, &request_context, &response);

            context.update(response.extracted_variables.clone());
            responses.push(response);
        }

        Ok(WorkflowResult {
            responses,
            final_variables: context.variables,
        })
    }
}
