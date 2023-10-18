use jsonpath::Selector;
use log::debug;
use reqwest::{header::HeaderMap, Client, RequestBuilder, StatusCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simplelog::{error, info};
use std::{collections::HashMap, fs, str::FromStr};

#[derive(Serialize, Deserialize, Debug, JsonSchema, strum::Display, Clone)]
enum Method {
    #[serde(alias = "get")]
    GET,
    #[serde(alias = "post")]
    POST,
    #[serde(alias = "put")]
    PUT,
    #[serde(alias = "delete")]
    DELETE,
    #[serde(alias = "patch")]
    PATCH,
    #[serde(alias = "head")]
    HEAD,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(untagged)]
enum ParamValue {
    StringParam(String),
    NumberParam(f32),
    BoolParam(bool),
    ListParam(Vec<serde_json::Value>),
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(untagged)]
enum Body {
    File { file: String },
    Content { content: String, trim: Option<bool> },
}

impl Body {
    fn content(&self) -> Vec<u8> {
        match self {
            Body::File { file } => fs::read(file).unwrap(),
            Body::Content { content, trim } => {
                let value = if trim.unwrap_or(false) {
                    content.trim()
                } else {
                    content
                };
                value.as_bytes().to_vec()
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(tag = "type")]
pub enum Authentication {
    #[serde(rename = "basic")]
    Basic {
        username: String,
        password: Option<String>,
    },

    #[serde(rename = "bearer")]
    Bearer { token: String },
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
struct Request {
    name: String,
    uri: String,
    method: Method,
    #[serde(rename = "queryParams")]
    query_params: Option<HashMap<String, ParamValue>>,
    headers: Option<HashMap<String, String>>,
    body: Option<Body>,
    authentication: Option<Authentication>,
    extractors: Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct ApiSpec {
    requests: Vec<Request>,
}

#[derive(Debug, Clone)]
pub struct ApiSpecExecutionContext {
    pub variables: HashMap<String, Option<String>>,
    client: Client,
}

impl ApiSpecExecutionContext {
    fn new(client: Client, variables: HashMap<String, Option<String>>) -> ApiSpecExecutionContext {
        ApiSpecExecutionContext {
            variables: variables,
            client: client,
        }
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

impl Request {
    fn final_uri(&self, context: &ApiSpecExecutionContext) -> String {
        replace_variables(&self.uri, &context.variables)
    }

    fn request(&self, context: &ApiSpecExecutionContext) -> anyhow::Result<RequestBuilder> {
        let base = match &self.method {
            Method::GET => context.client.get(self.final_uri(context)),
            Method::POST => context.client.post(self.final_uri(context)),
            Method::PUT => context.client.put(self.final_uri(context)),
            Method::DELETE => context.client.delete(self.final_uri(context)),
            Method::PATCH => context.client.patch(self.final_uri(context)),
            Method::HEAD => context.client.head(self.final_uri(context)),
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

async fn execute_request(
    request: Request,
    context: &ApiSpecExecutionContext,
) -> anyhow::Result<ApiSpecExecutionContext> {
    let http_request = request.request(&context)?.build()?;
    let res = context.client.execute(http_request).await?;
    let status = res.status();
    let headers = res.headers().clone();

    let body_string = res.text().await?;

    let json_value: Option<serde_json::Value> = match headers.get("content-type") {
        Some(json_content_type)
            if json_content_type
                .to_str()
                .expect("header should be a string")
                .to_lowercase()
                .starts_with("application/json") =>
        {
            Some(serde_json::from_str(&body_string)?)
        }
        _ => None,
    };

    let extracted_variables: HashMap<String, Option<String>> = match json_value.as_ref() {
        Some(json) => {
            let mut extracted_vals: HashMap<String, Option<String>> = HashMap::new();
            if let Some(extractors) = request.extractors.clone() {
                for (k, v) in extractors {
                    let s = Selector::new(&v).expect(&format!("Invalid jsonpath for {}", &k));
                    let v = s
                        .find(&json)
                        .flat_map(|v| match v {
                            v if v.is_string() => v.as_str().map(|v| v.to_string()),
                            v => Some(v.to_string()),
                        })
                        .next();

                    extracted_vals.insert(k.clone(), v);
                }
            }
            extracted_vals
        }
        None => HashMap::new(),
    };

    print_response(
        request.final_uri(context),
        status,
        json_value,
        body_string,
        request,
        headers,
        extracted_variables.clone(),
    )?;

    let mut current_variables: HashMap<String, Option<String>> = context.variables.clone();
    current_variables.extend(extracted_variables);

    Ok(ApiSpecExecutionContext::new(
        context.client.clone(),
        current_variables,
    ))
}

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
}

pub struct ExecutionResult {
    pub context: ApiSpecExecutionContext,
    pub last_error: Option<anyhow::Error>,
}

pub async fn execute_spec(
    api_spec: ApiSpec,
    initial_variables: HashMap<String, Option<String>>,
) -> anyhow::Result<ExecutionResult> {
    debug!("executing api_spec: {:?}", api_spec);
    let client = reqwest::Client::new();
    let mut context: ApiSpecExecutionContext =
        ApiSpecExecutionContext::new(client, initial_variables);
    let mut last_error: Option<anyhow::Error> = None;

    for request in api_spec.requests {
        let result = execute_request(request, &context).await;
        match result {
            Ok(new_context) => context = new_context,
            Err(err) => {
                error!("error executing request: {}", err);
                last_error = Some(err);
                break;
            }
        }
    }

    Ok(ExecutionResult {
        context: context,
        last_error: last_error,
    })
}

#[cfg(test)]
mod tests {
    use crate::climan::{execute_spec, ApiSpec};
    use httpmock::prelude::*;
    use std::collections::HashMap;
    use test_log::test;

    #[test(tokio::test)]
    async fn should_execute_spec() -> anyhow::Result<()> {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/get");
            then.status(200)
                .header("content-type", "text/plain")
                .body("ok");
        });

        server.mock(|when, then| {
            when.method(POST).path("/post");
            then.status(200)
                .header("content-type", "application/json")
                .body(include_str!("../tests/echo.json"));
        });

        let test_spec = include_str!("../tests/test.yaml").replace(
            "https://postman-echo.com",
            format!("http://{}:{}", server.host(), server.port()).as_str(),
        );

        let api_spec: ApiSpec = serde_yaml::from_str(&test_spec)?;
        let result = execute_spec(api_spec, HashMap::new()).await?;

        assert_eq!(result.last_error.is_none(), true);
        Ok(())
    }
}
