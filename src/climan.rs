pub mod model;
pub mod request;
pub mod workflow;

#[cfg(test)]
mod tests {
    use crate::climan::workflow::Workflow;
    use httpmock::prelude::*;
    use std::collections::HashMap;
    use test_log::test;

    #[test(tokio::test)]
    async fn should_execute_workflow() -> anyhow::Result<()> {
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

        let test_spec = include_str!("../tests/workflow.yaml").replace(
            "https://postman-echo.com",
            format!("http://{}:{}", server.host(), server.port()).as_str(),
        );

        let client = reqwest::Client::new();
        let workflow: Workflow = serde_yaml::from_str(&test_spec)?;
        let result = workflow
            .execute(&client, HashMap::new(), &|_, _| (), &|_, _, _| ())
            .await;

        assert!(result.is_ok());
        Ok(())
    }
}
