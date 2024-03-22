use std::{borrow::Borrow, collections::HashMap, str::FromStr, time::Duration};

use minijinja::Environment;
use reqwest::Client;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::model::*;

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct Request {
    pub name: String,
    pub uri: String,
    pub method: Method,
    #[serde(rename = "queryParams")]
    pub query_params: Option<HashMap<String, ParamValue>>,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<Body>,
    pub authentication: Option<Authentication>,
    pub extractors: Option<HashMap<String, String>>,
}

pub struct RequestContext<'v> {
    pub variables: &'v HashMap<String, Option<String>>,
    pub uri: String,
    pub method: Method,
    pub query_params: HashMap<&'v String, String>,
    pub headers: HashMap<&'v String, String>,
    pub body: Option<String>,
}

fn replace_variables(string_value: &str, variables: &HashMap<String, Option<String>>) -> String {
    match Environment::new().render_str(string_value, variables) {
        Ok(value) => value,
        Err(e) => {
            log::error!("Error while replacing variables: {}", e);
            string_value.to_string()
        }
    }
}

#[derive(Debug)]
pub struct Response {
    pub status_code: u16,
    pub time_to_headers: Duration,
    pub time_total: Duration,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub extracted_variables: HashMap<String, Option<String>>,
}

impl Request {
    pub async fn execute<'v>(
        &self,
        client: &Client,
        variables: &'v HashMap<String, Option<String>>,
        request_action: impl Fn(&Request, &RequestContext),
        response_action: impl Fn(&Request, &RequestContext, &Response),
    ) -> anyhow::Result<Response> {
        let (ctx, http_request) = self.request(client, variables)?;

        request_action(self, &ctx);
        let start_ts = std::time::Instant::now();
        let res = client.execute(http_request).await?;
        let headers_ts = std::time::Instant::now();

        let status = res.status().as_u16();
        let headers = res
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect::<HashMap<String, String>>();

        let is_json = res
            .headers()
            .get("content-type")
            .map(|content_type| {
                content_type
                    .to_str()
                    .expect("Content type is not a string")
                    .to_lowercase()
                    .starts_with("application/json")
            })
            .unwrap_or(false);

        let body_string = res.text().await?;
        let end_ts = std::time::Instant::now();

        let json_value: Option<serde_json::Value> = if is_json {
            Some(serde_json::from_str(&body_string)?)
        } else {
            None
        };

        let extracted_variables: HashMap<String, Option<String>> = match json_value.as_ref() {
            Some(json) => self.extract_variables(json),
            None => HashMap::new(),
        };

        let time_to_headers = headers_ts.duration_since(start_ts);
        let time_to_end = end_ts.duration_since(start_ts);

        let response = Response {
            status_code: status,
            time_to_headers,
            time_total: time_to_end,
            headers,
            body: body_string,
            extracted_variables,
        };

        response_action(self, &ctx, &response);

        Ok(response)
    }

    fn extract_variables(&self, json: &serde_json::Value) -> HashMap<String, Option<String>> {
        if let Some(extractors) = &self.extractors {
            let mut extracted_vals: HashMap<String, Option<String>> = HashMap::new();
            for (name, path) in extractors {
                let s = jsonpath::Selector::new(path)
                    .unwrap_or_else(|_| panic!("Invalid jsonpath for {}", &name));
                let v = s
                    .find(json)
                    .flat_map(|v| match v {
                        v if v.is_string() => v.as_str().map(|v| v.to_string()),
                        v => Some(v.to_string()),
                    })
                    .next();

                extracted_vals.insert(name.to_string(), v);
            }
            extracted_vals
        } else {
            HashMap::new()
        }
    }

    fn request<'v>(
        &'v self,
        client: &Client,
        variables: &'v HashMap<String, Option<String>>,
    ) -> anyhow::Result<(RequestContext<'v>, reqwest::Request)> {
        let final_uri = replace_variables(&self.uri, variables);

        let mut request_builder = match &self.method {
            Method::Get => client.get(&final_uri),
            Method::Post => client.post(&final_uri),
            Method::Put => client.put(&final_uri),
            Method::Delete => client.delete(&final_uri),
            Method::Patch => client.patch(&final_uri),
            Method::Head => client.head(&final_uri),
        };

        let final_query_params = if let Some(query_params) = &self.query_params {
            let params: Vec<(&String, String)> = query_params
                .iter()
                .flat_map(|(k, vs)| match vs {
                    ParamValue::StringParam(v) => {
                        vec![(k, replace_variables(v, variables))]
                    }
                    ParamValue::NumberParam(v) => {
                        vec![(k, replace_variables(&v.to_string(), variables))]
                    }
                    ParamValue::BoolParam(v) => {
                        vec![(k, replace_variables(&v.to_string(), variables))]
                    }
                    ParamValue::ListParam(vs) => vs
                        .iter()
                        .map(|v| (k, replace_variables(&v.to_string(), variables)))
                        .collect(),
                })
                .collect();

            HashMap::from_iter(params)
        } else {
            HashMap::new()
        };
        request_builder = request_builder.query(&final_query_params);

        let final_headers = if let Some(headers) = &self.headers {
            let header_it = headers
                .iter()
                .map(|(k, v)| (k, replace_variables(v, variables)));

            HashMap::from_iter(header_it)
        } else {
            HashMap::new()
        };
        request_builder = request_builder.headers(reqwest::header::HeaderMap::from_iter(
            final_headers.iter().map(|(k, v)| {
                (
                    reqwest::header::HeaderName::from_str(k).unwrap(),
                    reqwest::header::HeaderValue::from_str(v).unwrap(),
                )
            }),
        ));

        let final_body = self.body.as_ref().map(|body| {
            let body_string = String::from_utf8_lossy(&body.content()).to_string();
            replace_variables(&body_string, variables)
        });

        if let Some(body) = final_body.borrow() {
            request_builder = request_builder.body(body.clone());
        }

        if let Some(authentication) = &self.authentication {
            match authentication {
                Authentication::Basic { username, password } => {
                    request_builder = request_builder.basic_auth(
                        replace_variables(username, variables),
                        password
                            .clone()
                            .map(|value| replace_variables(&value, variables)),
                    )
                }
                Authentication::Bearer { token } => {
                    request_builder =
                        request_builder.bearer_auth(replace_variables(token, variables))
                }
            }
        };

        let request_context: RequestContext<'v> = RequestContext {
            variables,
            uri: final_uri,
            method: self.method.clone(),
            query_params: final_query_params,
            headers: final_headers,
            body: final_body,
        };

        Ok((request_context, request_builder.build()?))
    }
}
