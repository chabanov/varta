//! Trusted, bounded loopback HTTP exchange. No redirects, cookies or ambient credentials.
use super::*;
use http_body_util::{BodyExt, Full};
use hyper::{Request, body::Bytes};
use hyper_util::rt::TokioIo;
use std::{sync::Arc, time::Duration};
#[derive(Clone)]
pub(crate) struct Upstream {
    pub ceiling: ApplicationCeiling,
    token: Arc<String>,
}
impl Upstream {
    pub fn new(ceiling: ApplicationCeiling) -> crate::preview::Result<Self> {
        ceiling.validate()?;
        let token = std::env::var(&ceiling.native_control_token_env).map_err(|_| {
            crate::preview::PreviewError::Invalid("native control credential unavailable")
        })?;
        if !crate::host_control::digest(&token) {
            return Err(crate::preview::PreviewError::Invalid(
                "native control credential",
            ));
        }
        Ok(Self {
            ceiling,
            token: Arc::new(token),
        })
    }
    pub async fn exchange(
        &self,
        path: &str,
        method: &str,
        body: Vec<u8>,
        headers: Vec<(String, String)>,
        mut cancel: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(u16, Vec<u8>), &'static str> {
        let run = async {
            let url = url::Url::parse(&self.ceiling.native_origin).map_err(|_| "unavailable")?;
            let addr = format!(
                "{}:{}",
                url.host_str().ok_or("unavailable")?,
                url.port().ok_or("unavailable")?
            );
            let socket = tokio::net::TcpStream::connect(&addr)
                .await
                .map_err(|_| "unavailable")?;
            let (mut sender, connection) =
                hyper::client::conn::http1::handshake(TokioIo::new(socket))
                    .await
                    .map_err(|_| "unavailable")?;
            let mut request = Request::builder()
                .method(method)
                .uri(path)
                .header("host", addr)
                .header("authorization", format!("Bearer {}", self.token))
                .header("connection", "close");
            for (k, v) in headers {
                request = request.header(k, v);
            }
            let request = request
                .body(Full::new(Bytes::from(body)))
                .map_err(|_| "invalid_request")?;
            let work = async {
                let mut response = sender
                    .send_request(request)
                    .await
                    .map_err(|_| "unavailable")?;
                let status = response.status().as_u16();
                if !response
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|v| {
                        matches!(v, "application/json" | "application/json; charset=utf-8")
                    })
                {
                    return Err("invalid_upstream");
                }
                let mut out = Vec::new();
                while let Some(frame) = response.body_mut().frame().await {
                    let frame = frame.map_err(|_| "unavailable")?;
                    if let Some(b) = frame.data_ref() {
                        if b.len() > 256 * 1024 - out.len() {
                            return Err("body_limit");
                        }
                        out.extend_from_slice(b);
                    }
                }
                Ok((status, out))
            };
            tokio::pin!(connection);
            tokio::pin!(work);
            tokio::select! {biased;r=&mut work=>r,done=&mut connection=>{if done.is_err(){Err("unavailable")}else{work.await}}}
        };
        if *cancel.borrow() {
            return Err("ended");
        }
        tokio::select! {biased;_ = cancel.changed()=>Err("ended"),r=tokio::time::timeout(Duration::from_secs(2),run)=>r.unwrap_or(Err("unavailable"))}
    }
    pub async fn scope(&self, id: &str) -> Result<NativeScope, &'static str> {
        if !crate::host_control::digest(id) {
            return Err("invalid_scope");
        }
        let (tx, rx) = tokio::sync::watch::channel(false);
        let body = serde_json::to_vec(&serde_json::json!({"delegationId":id}))
            .map_err(|_| "invalid_scope")?;
        let (status, bytes) = self
            .exchange(
                "/__varta/session-delegation/v1/scope",
                "POST",
                body,
                vec![("content-type".into(), "application/json".into())],
                rx,
            )
            .await?;
        drop(tx);
        if status != 200 {
            return Err("ended");
        }
        let scope: NativeScope = serde_json::from_slice(&bytes).map_err(|_| "invalid_scope")?;
        if scope.schema_version != 1
            || scope.id != id
            || !crate::host_control::key_valid(&scope.binding_id)
            || !crate::host_control::digest(&scope.context_sha256)
            || !crate::host_control::digest(&scope.parent_generation)
            || scope.state != "active"
            || scope.expires_at_ms <= crate::host_control::now_ms()
            || scope.path_prefix != self.ceiling.path_prefix
            || scope.methods.is_empty()
            || scope.methods.windows(2).any(|p| p[0] >= p[1])
            || scope
                .methods
                .iter()
                .any(|m| !matches!(m.as_str(), "GET" | "PATCH" | "POST"))
        {
            return Err("ended");
        }
        Ok(scope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    fn client(address: std::net::SocketAddr) -> Upstream {
        let origin = format!("http://{address}");
        let ceiling:ApplicationCeiling=serde_json::from_value(serde_json::json!({"host":{"schemaVersion":1,"controlListen":"127.0.0.1:0","publicListen":"[::1]:8802","protectedOrigins":[origin],"maxImports":1,"maxRuns":1,"maxRunSeconds":120,"importSeconds":60,"retentionSeconds":300,"validationDeadlineMs":15000,"cpuSeconds":10,"addressSpaceBytes":4294967296u64,"budgets":{}},"publicOrigin":"http://localhost:8802","nativeOrigin":origin,"nativeControlTokenEnv":"TEST_NATIVE_CONTROL","pathPrefix":"/private"})).unwrap();
        ceiling.validate().unwrap();
        Upstream {
            ceiling,
            token: Arc::new("a".repeat(64)),
        }
    }
    async fn request(socket: &mut tokio::net::TcpStream) {
        let mut bytes = Vec::new();
        let mut chunk = [0; 4096];
        loop {
            let n = socket.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() < 8192);
            if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let length = head
                    .lines()
                    .find_map(|s| s.strip_prefix("content-length: "))
                    .unwrap_or("0")
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    assert!(head.contains(&format!("authorization: bearer {}", "a".repeat(64))));
                    assert!(!head.contains("cookie:"));
                    return;
                }
            }
        }
    }
    #[tokio::test]
    async fn application_upstream_reads_a_complete_response_when_peer_closes_immediately() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = client(listener.local_addr().unwrap());
        let id = "b".repeat(64);
        let body=serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"id":id,"bindingId":"binding-00000001","contextSha256":"c".repeat(64),"parentGeneration":"d".repeat(64),"state":"active","expiresAtMs":crate::host_control::now_ms()+100000,"pathPrefix":"/private","methods":["GET"]})).unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            request(&mut socket).await;
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await.unwrap();
            socket.write_all(&body).await.unwrap();
            socket.shutdown().await.unwrap();
        });
        let scope = client.scope(&id).await.unwrap();
        assert_eq!(scope.id, id);
        assert_eq!(scope.methods, ["GET"]);
        server.await.unwrap();
    }
    #[tokio::test]
    async fn application_upstream_stop_drops_the_socket_without_a_detached_driver() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = client(listener.local_addr().unwrap());
        let (entered, ready) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            request(&mut socket).await;
            entered.send(()).unwrap();
            let mut b = [0; 1];
            assert_eq!(socket.read(&mut b).await.unwrap(), 0);
        });
        let (tx, rx) = tokio::sync::watch::channel(false);
        let task =
            tokio::spawn(async move { client.exchange("/held", "GET", vec![], vec![], rx).await });
        ready.await.unwrap();
        tx.send_replace(true);
        assert_eq!(task.await.unwrap().unwrap_err(), "ended");
        tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
    }
}
