use std::{collections::HashMap, str::FromStr};

use reqwest::{Client, RequestBuilder};
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
}
impl RequestContext<'_> {
    pub fn new<'v>(variables: &'v HashMap<String, Option<String>>) -> RequestContext {
        RequestContext { variables }
    }
}

fn replace_variables(string_value: &str, variables: &HashMap<String, Option<String>>) -> String {
    let result: String =
        variables
            .into_iter()
            .fold(string_value.to_string(), |acc, (key, value)| match value {
                Some(value) => acc.replace(&format!("${}", key), &value),
                None => acc.to_string(),
            });
    result
}

#[derive(Debug)]
pub struct Response {
    pub uri: String,
    pub status_code: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub extracted_variables: HashMap<String, Option<String>>,
}

impl Request {
    fn final_uri(&self, context: &RequestContext) -> String {
        replace_variables(&self.uri, &context.variables)
    }

    pub async fn execute<'v>(
        &self,
        client: &Client,
        context: &RequestContext<'v>,
    ) -> anyhow::Result<Response> {
        let http_request = self.request(client, context)?.build()?;
        let uri = http_request.url().to_string();
        let res = client.execute(http_request).await?;

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
        let json_value: Option<serde_json::Value> = if is_json {
            Some(serde_json::from_str(&body_string)?)
        } else {
            None
        };

        let extracted_variables: HashMap<String, Option<String>> = match json_value.as_ref() {
            Some(json) => self.extract_variables(json),
            None => HashMap::new(),
        };

        let response = Response {
            uri: uri,
            status_code: status,
            headers: headers,
            body: body_string,
            extracted_variables: extracted_variables.clone(),
        };

        Ok(response)
    }

    fn extract_variables(&self, json: &serde_json::Value) -> HashMap<String, Option<String>> {
        if let Some(extractors) = &self.extractors {
            let mut extracted_vals: HashMap<String, Option<String>> = HashMap::new();
            for (name, path) in extractors {
                let s = jsonpath::Selector::new(&path)
                    .expect(&format!("Invalid jsonpath for {}", &name));
                let v = s
                    .find(&json)
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

    fn request(&self, client: &Client, context: &RequestContext) -> anyhow::Result<RequestBuilder> {
        let base = match &self.method {
            Method::GET => client.get(self.final_uri(context)),
            Method::POST => client.post(self.final_uri(context)),
            Method::PUT => client.put(self.final_uri(context)),
            Method::DELETE => client.delete(self.final_uri(context)),
            Method::PATCH => client.patch(self.final_uri(context)),
            Method::HEAD => client.head(self.final_uri(context)),
        };

        let request = if let Some(query_params) = &self.query_params {
            let params: Vec<(String, String)> = query_params
                .into_iter()
                .flat_map(|(k, vs)| match vs {
                    ParamValue::StringParam(v) => {
                        vec![(k.to_string(), replace_variables(v, &context.variables))]
                    }
                    ParamValue::NumberParam(v) => {
                        vec![(
                            k.to_string(),
                            replace_variables(&v.to_string(), &context.variables),
                        )]
                    }
                    ParamValue::BoolParam(v) => vec![(
                        k.to_string(),
                        replace_variables(&v.to_string(), &context.variables),
                    )],
                    ParamValue::ListParam(vs) => vs
                        .into_iter()
                        .map(|v| {
                            (
                                k.to_string(),
                                replace_variables(&v.to_string(), &context.variables),
                            )
                        })
                        .collect(),
                })
                .collect();

            base.query(&params)
        } else {
            base
        };

        let request = if let Some(headers) = &self.headers {
            let mut hm = reqwest::header::HeaderMap::new();

            for (k, v) in headers {
                hm.insert(
                    reqwest::header::HeaderName::from_str(k)?,
                    reqwest::header::HeaderValue::from_str(&replace_variables(
                        v,
                        &context.variables,
                    ))?,
                );
            }
            request.headers(hm)
        } else {
            request
        };

        let request = if let Some(body) = &self.body {
            let body_string = String::from_utf8_lossy(&body.content()).to_string();
            request.body(replace_variables(&body_string, &context.variables))
        } else {
            request
        };

        let request = if let Some(authentication) = &self.authentication {
            match authentication {
                Authentication::Basic { username, password } => request.basic_auth(
                    replace_variables(&username, &context.variables),
                    password
                        .clone()
                        .map(|value| replace_variables(&value, &context.variables)),
                ),
                Authentication::Bearer { token } => {
                    request.bearer_auth(replace_variables(&token, &context.variables))
                }
            }
        } else {
            request
        };

        Ok(request)
    }
}
