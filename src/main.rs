use std::collections::HashMap;
use std::io::{BufRead, BufReader, IoSlice, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path;
use std::thread;

use anyhow::{anyhow, Result};
use default::default;

fn main() {
    // You can use print statements as follows for debugging, they'll be visible when running tests.
    println!("Logs from your program will appear here!");

    // Uncomment this block to pass the first stage

    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("accepted new connection: {:?}", stream.peer_addr());
                thread::spawn(|| handle_connection(stream));
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum HttpMethod {
    Get,
    Post,
}

impl HttpMethod {
    fn parse(method: &str) -> Result<Self> {
        match method {
            "GET" => Ok(Self::Get),
            "POST" => Ok(Self::Post),
            _ => Err(anyhow!("Could not parse {} into HttpMethod", method)),
        }
    }
}

#[derive(Debug)]
struct Request {
    pub method: HttpMethod,
    pub path: String,
    pub http_version: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

fn valid_path(path: &str) -> bool {
    true
}

impl Request {
    fn from_stream(stream: &TcpStream) -> Result<Self> {
        let reader = BufReader::new(stream);
        let mut reader_lines = reader.lines();
        let request_line = reader_lines.next().unwrap().unwrap();

        let mut request_split = request_line.split_whitespace();
        let method = {
            let method_str = request_split
                .next()
                .ok_or(anyhow!("Couldn't parse request method. No data to parse."))?;
            HttpMethod::parse(&method_str)?
        };

        let path = {
            let path_str = request_split.next().ok_or(
                anyhow!("Couldn't get path from request. Http requests should be space separated, e.g. `<method> <path> <http_version>`, but no space was found."
            ))?;
            if !valid_path(path_str) {
                return Err(anyhow!("Path {} is not valid", path_str));
            };
            path_str.to_string()
        };

        let http_version = {
            let version = request_split.next().ok_or(
                anyhow!("Couldn't get http version from request. Http requests should be space separated, e.g. `<method> <path> <http_version>`, but there was no 3rd element when space separating."
            ))?;
            match version {
                "HTTP/1.1" => version.to_string(),
                _ => return Err(anyhow!("Bad HTTP version: {}", version)),
            }
        };
        
        let mut headers: HashMap<String, String> = HashMap::new();

        // Keep reading until we get an empty line
        let _ = reader_lines
            .take_while(|x| match x {
                Ok(v) => v != "",
                _ => false,
            })
            .map(|x| {
                let inner: String = x.ok().unwrap();
                let header_split = inner.split_once(": ").unwrap();
                headers.insert(header_split.0.to_string(), header_split.1.to_string());
            })
            .collect::<Vec<_>>();

        Ok(Self {
            method,
            path,
            http_version,
            headers,
            body: None,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
enum HttpCode {
    Ok,
    NotFound,
    InternalServerError,
    BadRequest,
}

impl HttpCode {
    fn to_tcp_format(&self) -> &'static str {
        match self {
            HttpCode::Ok => "200 OK",
            HttpCode::NotFound => "404 Not Found",
            HttpCode::InternalServerError => "500 Internal Error",
            HttpCode::BadRequest => "400 Bad Request",
        }
    }
}

#[derive(Debug)]
struct Response {
    pub http_code: HttpCode,
    pub headers: HashMap<String, String>,
    pub content: Option<String>,
}

impl Default for Response {
    fn default() -> Self {
        Self {
            http_code: HttpCode::Ok,
            headers: HashMap::new(),
            content: None,
        }
    }
}

impl Response {
    fn write_to_stream(self, mut stream: &TcpStream) -> Result<usize> {
        let mut stream_output = Vec::from([
            IoSlice::new(b"HTTP/1.1 "),
            IoSlice::new(self.http_code.to_tcp_format().as_bytes()),
            IoSlice::new(b"\r\n"),
        ]);

        for (k, v) in self.headers.iter() {
            stream_output.push(IoSlice::new(k.as_bytes()));
            stream_output.push(IoSlice::new(b": "));
            stream_output.push(IoSlice::new(v.as_bytes()));
            stream_output.push(IoSlice::new(b"\r\n"));
        }

        stream_output.push(IoSlice::new(b"\r\n"));

        self.content
            .as_ref()
            .map(|s| stream_output.push(IoSlice::new(s.as_bytes())));

        let write_result = stream.write_vectored(&stream_output);
        match write_result {
            Ok(n) => {
                println!("Sent {n} bytes back.");
                Ok(n)
            }
            Err(err) => Err(anyhow!("Could not write response to stream: {}", err)),
        }
    }
}

fn handle_connection(mut stream: TcpStream) {
    let request = Request::from_stream(&stream).unwrap();

    match request.path.as_str() {
        "/" => {
            Response::default().write_to_stream(&stream).unwrap();
        },
        path => {
            if path.starts_with("/echo") {
                let echo_word = path[1..].split_once('/').unwrap().1;

                let mut headers: HashMap<String, String> = HashMap::new();
                headers.insert("Content-Type".into(), "text/plain".into());
                headers.insert("Content-Length".into(), format!("{}", echo_word.len()));

                let response = Response {
                    http_code: HttpCode::Ok,
                    headers,
                    content: Some(echo_word.to_owned()),
                };

                response.write_to_stream(&stream).unwrap();
            } else if path.starts_with("/user-agent") {

                let mut headers: HashMap<String, String> = HashMap::new();

                headers.insert("Content-Type".into(), "text/plain".into());
                headers.insert("Content-Length".into(), format!("{}", request.headers.get("User-Agent").unwrap().len()));

                let response = Response {
                    http_code: HttpCode::Ok,
                    headers,
                    content: Some(request.headers.get("User-Agent").unwrap().to_owned()),
                };

                response.write_to_stream(&stream).unwrap();

            } else {
                stream
                    .write("HTTP/1.1 404 Not Found\r\n\r\n".as_bytes())
                    .unwrap();
            }
        }
    }
}
