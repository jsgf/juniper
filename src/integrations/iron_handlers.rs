//! Optional handlers for the Iron framework. Requires the `iron-handlers` feature enabled.

use iron::prelude::*;
use iron::middleware::Handler;
use iron::mime::Mime;
use iron::status;
use iron::method;

use std::collections::{HashMap, BTreeMap};

use rustc_serialize::json::{ToJson, Json};

use ::{InputValue, GraphQLType, RootNode, execute};

/// Handler that executes GraphQL queries in the given schema
///
/// The handler responds to GET requests and POST requests only. In GET
/// requests, the query should be supplied in the `query` URL parameter, e.g.
/// `http://localhost:3000/graphql?query={hero{name}}`.
///
/// POST requests support both queries and variables. POST a JSON document to
/// this endpoint containing the field `"query"` and optionally `"variables"`.
/// The variables should be a JSON object containing the variable to value
/// mapping.
pub struct GraphQLHandler<CtxFactory, Query, Mutation, CtxT>
    where CtxFactory: Fn(&mut Request) -> CtxT + Send + Sync + 'static,
          CtxT: Send + Sync + 'static,
          Query: GraphQLType<CtxT> + Send + Sync + 'static,
          Mutation: GraphQLType<CtxT> + Send + Sync + 'static,
{
    context_factory: CtxFactory,
    root_node: RootNode<CtxT, Query, Mutation>,
}

/// Handler that renders GraphiQL - a graphical query editor interface
pub struct GraphiQLHandler {
    graphql_url: String,
}

impl<CtxFactory, Query, Mutation, CtxT>
    GraphQLHandler<CtxFactory, Query, Mutation, CtxT>
    where CtxFactory: Fn(&mut Request) -> CtxT + Send + Sync + 'static,
          CtxT: Send + Sync + 'static,
          Query: GraphQLType<CtxT> + Send + Sync + 'static,
          Mutation: GraphQLType<CtxT> + Send + Sync + 'static,
{
    /// Build a new GraphQL handler
    ///
    /// The context factory will receive the Iron request object and is
    /// expected to construct a context object for the given schema. This can 
    /// be used to construct e.g. database connections or similar data that
    /// the schema needs to execute the query.
    pub fn new(context_factory: CtxFactory, query: Query, mutation: Mutation) -> Self {
        GraphQLHandler {
            context_factory: context_factory,
            root_node: RootNode::new(query, mutation),
        }
    }


    fn handle_get(&self, req: &mut Request) -> IronResult<Response> {
        let url = req.url.clone().into_generic_url();

        let mut query = None;
        let variables = HashMap::new();

        for (k, v) in url.query_pairs() {
            if k == "query" {
                query = Some(v.into_owned());
            }
        }

        let query = iexpect!(query);

        self.execute(req, &query, &variables)
    }

    fn handle_post(&self, req: &mut Request) -> IronResult<Response> {
        let json_data = itry!(Json::from_reader(&mut req.body));

        let json_obj = match json_data {
            Json::Object(o) => o,
            _ => return Ok(Response::with((status::BadRequest, "No JSON object was decoded"))),
        };

        let mut query = None;
        let mut variables = HashMap::new();

        for (k, v) in json_obj.into_iter() {
            if k == "query" {
                query = v.as_string().map(|s| s.to_owned());
            }
            else if k == "variables" {
                variables = match InputValue::from_json(v).to_object_value() {
                    Some(o) => o.into_iter().map(|(k, v)| (k.to_owned(), v.clone())).collect(),
                    _ => HashMap::new(),
                };
            }
        }

        let query = iexpect!(query);

        self.execute(req, &query, &variables)
    }

    fn execute(&self, req: &mut Request, query: &str, variables: &HashMap<String, InputValue>) -> IronResult<Response> {
        let context = (self.context_factory)(req);
        let result = execute(query, None, &self.root_node, variables, &context);

        let content_type = "application/json".parse::<Mime>().unwrap();

        match result {
            Ok((result, errors)) => {
                let mut map = BTreeMap::new();
                map.insert("data".to_owned(), result.to_json());
                if !errors.is_empty() {
                    map.insert("errors".to_owned(), errors.to_json());
                }

                let data = Json::Object(map);
                let json = data.pretty();

                Ok(Response::with((content_type, status::Ok, json.to_string())))
            }

            Err(err) => {
                let data = err.to_json();
                let json = data.pretty();

                Ok(Response::with((content_type, status::BadRequest, json.to_string())))
            }
        }
    }
}

impl GraphiQLHandler {
    /// Build a new GraphiQL handler targeting the specified URL.
    ///
    /// The provided URL should point to the URL of the attached `GraphQLHandler`. It can be
    /// relative, so a common value could be `"/graphql"`.
    pub fn new(graphql_url: &str) -> GraphiQLHandler {
        GraphiQLHandler {
            graphql_url: graphql_url.to_owned(),
        }
    }
}

impl<CtxFactory, Query, Mutation, CtxT>
    Handler
    for GraphQLHandler<CtxFactory, Query, Mutation, CtxT>
    where CtxFactory: Fn(&mut Request) -> CtxT + Send + Sync + 'static,
          CtxT: Send + Sync + 'static,
          Query: GraphQLType<CtxT> + Send + Sync + 'static,
          Mutation: GraphQLType<CtxT> + Send + Sync + 'static,
{
    fn handle(&self, req: &mut Request) -> IronResult<Response> {
        match req.method {
            method::Get => self.handle_get(req),
            method::Post => self.handle_post(req),
            _ => Ok(Response::with((status::MethodNotAllowed)))
        }
    }
}

impl Handler for GraphiQLHandler {
    fn handle(&self, _: &mut Request) -> IronResult<Response> {
        let content_type = "text/html".parse::<Mime>().unwrap();

        let stylesheet_source = r#"
        <style>
            html, body, #app {
                height: 100%;
                margin: 0;
                overflow: hidden;
                width: 100%;
            }
        </style>
        "#;

        let fetcher_source = r#"
        <script>
            function graphQLFetcher(params) {
                return fetch(GRAPHQL_URL, {
                    method: 'post',
                    headers: {
                        'Accept': 'application/json',
                        'Content-Type': 'application/json',
                    },
                    body: JSON.stringify({
                        query: params.query,
                        variables: JSON.parse(params.variables || '{}')
                    }),
                }).then(function (response) {
                    return response.text();
                }).then(function (body) {
                    try {
                        return JSON.parse(body);
                    } catch (error) {
                        return body;
                    }
                });
            }
            ReactDOM.render(
                React.createElement(GraphiQL, {
                    fetcher: graphQLFetcher,
                }),
                document.querySelector('#app'));
        </script>
        "#;

        let source = format!(r#"
<!DOCTYPE html>
<html>
    <head>
        <title>GraphQL</title>
        {stylesheet_source}
        <link rel="stylesheet" type="text/css" href="//cdnjs.cloudflare.com/ajax/libs/graphiql/0.7.3/graphiql.css">
    </head>
    <body>
        <div id="app"></div>

        <script src="//cdnjs.cloudflare.com/ajax/libs/fetch/1.0.0/fetch.js"></script>
        <script src="//cdnjs.cloudflare.com/ajax/libs/react/15.2.1/react.js"></script>
        <script src="//cdnjs.cloudflare.com/ajax/libs/react/15.2.1/react-dom.js"></script>
        <script src="//cdnjs.cloudflare.com/ajax/libs/graphiql/0.7.3/graphiql.js"></script>
        <script>var GRAPHQL_URL = '{graphql_url}';</script>
        {fetcher_source}
    </body>
</html>
"#,
            graphql_url = self.graphql_url,
            stylesheet_source = stylesheet_source,
            fetcher_source = fetcher_source);

        Ok(Response::with((content_type, status::Ok, source)))
    }
}


#[cfg(test)]
mod tests {
    use rustc_serialize::json::Json;
    
    use iron::prelude::*;
    use iron::status;
    use iron::headers;
    use iron_test::{request, response};
    use iron::{Handler, Headers};

    use ::tests::model::Database;

    use super::GraphQLHandler;

    fn context_factory(_: &mut Request) -> Database {
        Database::new()
    }

    fn make_handler() -> Box<Handler> {
        Box::new(GraphQLHandler::new(
            context_factory,
            Database::new(),
            (),
        ))
    }

    fn unwrap_json_response(resp: Response) -> Json {
        let result = response::extract_body_to_string(resp);

        Json::from_str(&result).expect("Could not parse JSON object")
    }

    #[test]
    fn test_simple_get() {
        let response = request::get(
            "http://localhost:3000/?query={hero{name}}",
            Headers::new(),
            &make_handler())
            .expect("Unexpected IronError");

        assert_eq!(response.status, Some(status::Ok));
        assert_eq!(response.headers.get::<headers::ContentType>(),
                   Some(&headers::ContentType::json()));

        let json = unwrap_json_response(response);

        assert_eq!(
            json,
            Json::from_str(r#"{"data": {"hero": {"name": "R2-D2"}}}"#)
                .expect("Invalid JSON constant in test"));
    }

    #[test]
    fn test_simple_post() {
        let response = request::post(
            "http://localhost:3000/",
            Headers::new(),
            r#"{"query": "{hero{name}}"}"#,
            &make_handler())
            .expect("Unexpected IronError");

        assert_eq!(response.status, Some(status::Ok));
        assert_eq!(response.headers.get::<headers::ContentType>(),
                   Some(&headers::ContentType::json()));

        let json = unwrap_json_response(response);

        assert_eq!(
            json,
            Json::from_str(r#"{"data": {"hero": {"name": "R2-D2"}}}"#)
                .expect("Invalid JSON constant in test"));
    }

    #[test]
    fn test_unsupported_method() {
        let response = request::options(
            "http://localhost:3000/?query={hero{name}}",
            Headers::new(),
            &make_handler())
            .expect("Unexpected IronError");

        assert_eq!(response.status, Some(status::MethodNotAllowed));
    }
}
