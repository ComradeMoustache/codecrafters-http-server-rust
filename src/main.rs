use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path;

use anyhow::{anyhow, Result};

fn main() {
    // You can use print statements as follows for debugging, they'll be visible when running tests.
    println!("Logs from your program will appear here!");

    // Uncomment this block to pass the first stage

    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                //println!("accepted new connection");
                handle_connection(stream);
                //println!("Returned 200 OK to {}", stream.peer_addr());
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
            _ => Err(anyhow!("Could not parse {} into HttpMethod", method))
        }
    }
}

#[derive(Debug)]
struct Request<'a> {
    pub method: HttpMethod,
    pub path: &'a str,
    pub http_version: &'a str,
    // Host: localhost
    pub headers: HashMap<&'a str, &'a str>,
    pub body: Option<&'a str>,
}

fn valid_path(path: &str) -> bool {
    true
}

impl<'a> Request<'a> {
    fn parse(request_line: &'a str) -> Result<Self> {
        let mut request_split = request_line.split_whitespace();
        let method = {
            let method_str = request_split.next().ok_or(anyhow!("Couldn't parse request method. No data to parse."))?;
            HttpMethod::parse(&method_str)?
        };

        let path = {
            let path_str = request_split.next().ok_or(
                anyhow!("Couldn't get path from request. Http requests should be space separated, e.g. `<method> <path> <http_version>`, but no space was found."
            ))?;
            if !valid_path(path_str) {
                return Err(anyhow!("Path {} is not valid", path_str))
            };
            path_str
        };

        let http_version = {
            let version = request_split.next().ok_or(
                anyhow!("Couldn't get http version from request. Http requests should be space separated, e.g. `<method> <path> <http_version>`, but there was no 3rd element when space separating."
            ))?;
            match version {
                "HTTP/1.1" => version,
                _ => return Err(anyhow!("Bad HTTP version: {}", version))
            }
        };

        Ok(Self {
            method,
            path,
            http_version,
            headers: HashMap::new(),
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

#[derive(Debug)]
struct Response {
    pub response: HttpCode,
    pub content: String,
}

fn handle_connection(mut stream: TcpStream) {
    let reader = BufReader::new(&stream);

    // We should never be getting empting requests (at the moment at least..)
    let raw_stream = reader.lines().next().unwrap().unwrap();
    let request = Request::parse(&raw_stream).unwrap();

    match request.path {
        "/" => {
            stream.write("HTTP/1.1 200 OK\r\n\r\n".as_bytes()).unwrap();
        }
        _ => {
            stream.write("HTTP/1.1 404 Not Found\r\n\r\n".as_bytes()).unwrap();
        }

    }
    // loop {
    //     let result = stream.read_line(&mut buffer);
    //     match result {
    //         Ok(n) => println!("Recieved {} bytes", n),
    //         _ => {}
    //     }
    // }

}
