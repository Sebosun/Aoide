#![allow(dead_code)] // Shared by several test crates; each uses a subset.
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    task::JoinHandle,
};

#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
    pub chunked: bool,
    pub stall: Option<Stall>,
    pub extra_headers: Vec<(String, String)>,
    pub declared_length: Option<usize>,
}

#[derive(Clone, Copy)]
pub enum Stall {
    BeforeHeaders,
    AfterHeaders,
}

impl Response {
    pub fn wav(body: Vec<u8>) -> Self {
        Self {
            status: 200,
            content_type: "audio/wav".into(),
            body,
            chunked: false,
            stall: None,
            extra_headers: Vec::new(),
            declared_length: None,
        }
    }
    pub fn json(body: &str) -> Self {
        Self {
            status: 200,
            content_type: "application/json".into(),
            body: body.as_bytes().to_vec(),
            chunked: false,
            stall: None,
            extra_headers: Vec::new(),
            declared_length: None,
        }
    }
}

pub fn wav() -> Vec<u8> {
    // Original fixture: mono, 24 kHz, signed PCM16, 64 frames (172 bytes total).
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&164u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&24000u32.to_le_bytes());
    bytes.extend_from_slice(&48000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&128u32.to_le_bytes());
    for value in -32i16..32 {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

pub struct Server {
    pub url: String,
    pub count: Arc<AtomicUsize>,
    requests: mpsc::UnboundedReceiver<Request>,
    task: JoinHandle<()>,
}

impl Server {
    pub async fn start(response: Response) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, requests) = mpsc::unbounded_channel();
        let count = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&count);
        let task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_request(&mut stream).await;
                counter.fetch_add(1, Ordering::SeqCst);
                if tx.send(request).is_err() {
                    break;
                }
                // Clients deliberately disconnect on oversized responses.
                let _ = write_response(&mut stream, &response).await;
            }
        });
        Self {
            url,
            count,
            requests,
            task,
        }
    }

    pub async fn request(&mut self) -> Request {
        tokio::time::timeout(std::time::Duration::from_secs(5), self.requests.recv())
            .await
            .unwrap()
            .unwrap()
    }
}

async fn write_response(stream: &mut TcpStream, response: &Response) -> std::io::Result<()> {
    if matches!(response.stall, Some(Stall::BeforeHeaders)) {
        std::future::pending::<()>().await;
    }
    let framing = if response.chunked {
        "Transfer-Encoding: chunked".into()
    } else {
        format!(
            "Content-Length: {}",
            response.declared_length.unwrap_or(response.body.len())
        )
    };
    let extra_headers = response
        .extra_headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect::<String>();
    let head = format!("HTTP/1.1 {} Test\r\nContent-Type: {}\r\n{framing}\r\n{extra_headers}Connection: close\r\n\r\n", response.status, response.content_type);
    stream.write_all(head.as_bytes()).await?;
    if matches!(response.stall, Some(Stall::AfterHeaders)) {
        std::future::pending::<()>().await;
    }
    if response.chunked {
        for chunk in response.body.chunks(7) {
            stream
                .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
                .await?;
            stream.write_all(chunk).await?;
            stream.write_all(b"\r\n").await?;
        }
        stream.write_all(b"0\r\n\r\n").await?;
    } else {
        stream.write_all(&response.body).await?;
    }
    Ok(())
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn read_request(stream: &mut TcpStream) -> Request {
    let mut bytes = Vec::new();
    let header_end = loop {
        let mut buffer = [0; 1024];
        let count = stream.read(&mut buffer).await.unwrap();
        assert!(count > 0, "connection closed before headers");
        bytes.extend_from_slice(&buffer[..count]);
        assert!(bytes.len() < 2 * 1024 * 1024, "oversized test request");
        if let Some(index) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let header = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
    let mut lines = header.lines();
    let mut first = lines.next().unwrap().split_whitespace();
    let method = first.next().unwrap().to_owned();
    let path = first.next().unwrap().to_owned();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.to_ascii_lowercase(), value.trim().to_owned()))
        .collect::<BTreeMap<_, _>>();
    let length = headers
        .get("content-length")
        .map_or(0, |s| s.parse::<usize>().unwrap());
    assert!(length < 2 * 1024 * 1024);
    while bytes.len() < header_end + length {
        let mut buffer = [0; 1024];
        let count = stream.read(&mut buffer).await.unwrap();
        assert!(count > 0, "connection closed before request body");
        bytes.extend_from_slice(&buffer[..count]);
    }
    Request {
        method,
        path,
        headers,
        body: bytes[header_end..header_end + length].to_vec(),
    }
}
