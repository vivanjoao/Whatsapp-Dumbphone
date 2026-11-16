//! Network deadlines apply inside the blocking HTTP worker, including streaming
//! body reads. Cancelling its async caller alone cannot stop that worker.

use std::time::Duration;
use whatsapp_rust::http::UreqHttpClient;

pub fn new() -> UreqHttpClient {
    configured(
        Duration::from_secs(5),
        Duration::from_secs(10),
        Duration::from_secs(30),
    )
}

fn configured(connect: Duration, read: Duration, total: Duration) -> UreqHttpClient {
    let agent = ureq::config::Config::builder()
        .timeout_resolve(Some(connect))
        .timeout_connect(Some(connect))
        .timeout_per_read(Some(read))
        .timeout_per_write(Some(read))
        .timeout_recv_response(Some(read))
        .timeout_recv_body(Some(total))
        .timeout_global(Some(total))
        // Preserve the SDK's small connection pool and buffers. Its response
        // size cap and non-2xx status handling also remain in force.
        .input_buffer_size(16 * 1024)
        .output_buffer_size(16 * 1024)
        .max_idle_connections(3)
        .max_idle_connections_per_host(2)
        .build()
        .into();
    UreqHttpClient::with_agent(agent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
        time::Instant,
    };
    use whatsapp_rust::wacore::net::{HttpClient, HttpRequest};

    // A local peer that deliberately keeps headers/body unfinished. Dropping
    // this fixture releases and joins the peer even when an assertion fails.
    struct Peer {
        url: String,
        stop: mpsc::Sender<()>,
        worker: Option<thread::JoinHandle<()>>,
    }

    impl Peer {
        fn new(response: &'static [u8], trickle: bool) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let (stop, stopped) = mpsc::channel();
            let worker = thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    if socket.read_exact(&mut byte).is_err() {
                        return;
                    }
                    request.push(byte[0]);
                }
                if socket.write_all(response).is_err() {
                    return;
                }
                // A broken client deadline must fail the elapsed-time assertion,
                // rather than hang the entire test suite indefinitely.
                let started = Instant::now();
                while started.elapsed() < Duration::from_secs(3) {
                    match stopped.recv_timeout(Duration::from_millis(20)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if trickle && socket.write_all(b"x").is_err() {
                                break;
                            }
                        }
                    }
                }
            });
            Self {
                url,
                stop,
                worker: Some(worker),
            }
        }
    }

    impl Drop for Peer {
        fn drop(&mut self) {
            let _ = self.stop.send(());
            self.worker.take().unwrap().join().unwrap();
        }
    }

    fn short_client() -> UreqHttpClient {
        configured(
            Duration::from_millis(150),
            Duration::from_millis(150),
            Duration::from_millis(500),
        )
    }

    #[tokio::test]
    async fn incomplete_headers_time_out_inside_the_blocking_worker() {
        let peer = Peer::new(b"HTTP/1.1 200 OK\r\n", false);
        let started = Instant::now();
        let error = short_client()
            .execute(HttpRequest::get(&peer.url))
            .await
            .unwrap_err();
        assert!(format!("{error:#}").to_lowercase().contains("timeout"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn incomplete_buffered_body_times_out() {
        let peer = Peer::new(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\nx", false);
        let started = Instant::now();
        assert!(
            short_client()
                .execute(HttpRequest::get(&peer.url))
                .await
                .is_err()
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn incomplete_streaming_body_times_out() {
        let peer = Peer::new(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\nx", false);
        let started = Instant::now();
        let mut response = short_client()
            .execute_streaming(HttpRequest::get(&peer.url))
            .unwrap();
        assert!(std::io::copy(&mut response.body, &mut std::io::sink()).is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn trickling_stream_cannot_restart_the_global_deadline() {
        let peer = Peer::new(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\nx", true);
        let started = Instant::now();
        let mut response = short_client()
            .execute_streaming(HttpRequest::get(&peer.url))
            .unwrap();
        assert!(std::io::copy(&mut response.body, &mut std::io::sink()).is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn completed_error_response_keeps_its_status() {
        let peer = Peer::new(
            b"HTTP/1.1 403 Forbidden\r\nContent-Length: 6\r\n\r\ndenied",
            false,
        );
        let response = new().execute(HttpRequest::get(&peer.url)).await.unwrap();
        assert_eq!(response.status_code, 403);
        assert_eq!(response.body, b"denied");
    }
}
