use axum::{
    Router,
    extract::{Request, State},
    http::{Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
};

pub const ADDRESS: &str = "127.0.0.1:34618";
pub const ORIGIN: &str = "http://127.0.0.1:34618";

pub fn protect(router: Router, token: String) -> Router {
    router.layer(middleware::from_fn_with_state(token, guard))
}

async fn guard(State(token): State<String>, request: Request, next: Next) -> Response {
    let host_ok = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        == Some(ADDRESS);
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok());
    let safe_method = matches!(*request.method(), Method::GET | Method::HEAD);
    if !host_ok
        || (origin.is_some() && origin != Some(ORIGIN))
        || (!safe_method && origin != Some(ORIGIN))
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if request.method() == Method::GET && request.uri().path() == format!("/_desktop/{token}") {
        let mut response = Redirect::to("/").into_response();
        response.headers_mut().insert(
            header::SET_COOKIE,
            format!("dbsaur_desktop={token}; HttpOnly; SameSite=Strict; Path=/")
                .parse()
                .unwrap(),
        );
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
        return response;
    }
    let cookie = format!("dbsaur_desktop={token}");
    let authenticated = request
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|value| value.split(';').any(|part| part.trim() == cookie));
    if !authenticated {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    response
        .headers_mut()
        .insert(header::X_FRAME_OPTIONS, "DENY".parse().unwrap());
    response
        .headers_mut()
        .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, routing::get};
    use tower::ServiceExt;

    async fn status(
        method: Method,
        path: &str,
        host: &str,
        origin: Option<&str>,
        cookie: Option<&str>,
    ) -> StatusCode {
        let router = protect(
            Router::new().route(
                "/api/health",
                get(|| async { "ok" }).post(|| async { "ok" }),
            ),
            "secret".into(),
        );
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, host);
        if let Some(origin) = origin {
            request = request.header(header::ORIGIN, origin);
        }
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        router
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn desktop_session_and_origin_are_required() {
        assert_eq!(
            status(Method::GET, "/api/health", ADDRESS, None, None).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(
                Method::GET,
                "/api/health",
                ADDRESS,
                None,
                Some("dbsaur_desktop=wrong")
            )
            .await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(
                Method::GET,
                "/api/health",
                ADDRESS,
                None,
                Some("dbsaur_desktop=secret")
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            status(
                Method::POST,
                "/api/health",
                ADDRESS,
                Some(ORIGIN),
                Some("dbsaur_desktop=secret")
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            status(
                Method::POST,
                "/api/health",
                ADDRESS,
                None,
                Some("dbsaur_desktop=secret")
            )
            .await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(
                Method::POST,
                "/api/health",
                ADDRESS,
                Some("https://attacker.example"),
                Some("dbsaur_desktop=secret")
            )
            .await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(
                Method::GET,
                "/api/health",
                "attacker.example:34618",
                None,
                Some("dbsaur_desktop=secret")
            )
            .await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(Method::GET, "/_desktop/wrong", ADDRESS, None, None).await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn bootstrap_sets_private_cookie_and_redirects() {
        let router = protect(Router::new(), "secret".into());
        let request = Request::builder()
            .uri("/_desktop/secret")
            .header(header::HOST, ADDRESS)
            .body(Body::empty())
            .unwrap();
        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()[header::LOCATION], "/");
        assert_eq!(
            response.headers()[header::SET_COOKIE],
            "dbsaur_desktop=secret; HttpOnly; SameSite=Strict; Path=/"
        );
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }
}
