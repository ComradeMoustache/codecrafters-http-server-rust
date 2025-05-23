use clap::Parser;
use core::panic;
use std::collections::{HashMap};
use std::fs::File;
use std::io::{IoSlice, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;
use std::fs;

use anyhow::{anyhow, Result};

const DEFAULT_TIMEOUT: u8 = 5; // seconds
const END_OF_HEADER: &str = "\r\n\r\n";
const CONTENT_TYPE_HEADER: &str = "Content-Type";
const CONTENT_LENGTH_HEADER: &str = "Content-Length";
const DEFAULT_FILES_DIR: &str = "/tmp/rust-http-server/";

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    directory: Option<std::path::PathBuf>,
}

fn main() {
    // You can use print statements as follows for debugging, they'll be visible when running tests.
    println!("Logs from your program will appear here!");

    // Uncomment this block to pass the first stage
    let mut config = Cli::parse();

    match &config.directory {
        None => config.directory = Some(DEFAULT_FILES_DIR.into()),
        Some(dir) => {
            if !dir.starts_with("/tmp/") {
                panic!("Expecting server directory to be stored in /tmp/");
            }
        }
    }

    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    std::thread::scope(|scope| {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    println!("accepted new connection: {:?}", stream.peer_addr());
                    scope.spawn(|| handle_connection(stream, &config));
                }
                Err(e) => {
                    println!("error: {}", e);
                }
            }
        }
    })
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
struct Request<'a> {
    pub method: HttpMethod,
    pub path: String,
    pub http_version: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,

    vars: Option<HashMap<&'a str, &'a str>>
}

/*

I think that I can iterate through thet string until I get a buffer with `\r\n\r\n`
which marks the end of the header. Then I need to pull in whatever is in Content-Length
header.

- Do post requests always have a Content-Length header?
- Are headers always capitalised with dashes?

*/

impl Request<'_> {
    fn from_stream(mut stream: &TcpStream) -> Result<Self> {
        // 1KiB array
        let mut buffer = [0; 1024];
        let mut request = String::new();
        let mut parsed_request: Request;
        let mut returned_bytes: usize;

        stream
            .set_read_timeout(Some(Duration::from_secs(DEFAULT_TIMEOUT as u64)))
            .unwrap();

        /*
        Could look to use `.as_ref()` on the stream.

        Doing `.as_ref()` will consume the iterator but won't take ownership of it. So 
        it would allow me to do `.lines()` to get the header here.
        */
        loop {
            returned_bytes = stream.read(&mut buffer)?;
            println!("Bytes returned: {}", returned_bytes);

            if returned_bytes == 0 {
                break;
            }

            request.push_str(std::str::from_utf8(&buffer[..returned_bytes]).unwrap());

            if request.contains(END_OF_HEADER) {
                println!("End of header found.");
                break;
            }
        }

        let start_string_length: usize;
        // Get the string up to the end of the header.
        if let Some((start_string, _)) = request.split_once(END_OF_HEADER) {
            start_string_length = start_string.len();
            parsed_request = Request::parse_up_to_header(&start_string)?;
        } else {
            return Err(anyhow!(
                "Couldn't find end of header, data recieved: {}.",
                request
            ));
        }

        let content_length: usize;
        // Now that I have a header, if there is a content-length header, keep reading
        // the stream until the data has been completely read in.
        if let Some(content_header_value) = parsed_request.headers.get("Content-Length") {
            match content_header_value.parse::<usize>() {
                Err(err) => {
                    return Err(anyhow!(
                        "Could not parse Content-Length header value `{}` to number, got error: {}",
                        content_header_value,
                        err
                    ))
                }
                Ok(length) => {
                    content_length = length;
                }
            }
        } else {
            eprintln!("No content length header set.");
            return Ok(parsed_request);
        }

        while request.len() < (content_length + start_string_length + END_OF_HEADER.len()) {
            returned_bytes = stream.read(&mut buffer)?;
            println!("Bytes returned: {}", returned_bytes);

            if returned_bytes == 0 {
                break;
            }

            request.push_str(std::str::from_utf8(&buffer[..returned_bytes]).unwrap());
        }

        let (_, content) = request
            .split_once(END_OF_HEADER)
            .expect(format!("Could not find `{}` in string.", END_OF_HEADER).as_str());

        if content_length == content.len() {
            parsed_request.body = Some(content.into());
            return Ok(parsed_request);
        } else if content_length < content.len() {
            return Err(anyhow!(
                "More content data was sent, expected {} bytes but found {}",
                content_length,
                content.len()
            ));
        } else {
            return Err(anyhow!(
                "Not enough content data was sent, expected {} bytes but found {}",
                content.len(),
                content.len()
            ));
        }
    }

    fn parse_up_to_header(header_string: &str) -> Result<Self> {
        let mut reader_lines = header_string.lines();
        let method: HttpMethod;
        let path: String;
        let http_version: String;

        if let Some(request_line) = reader_lines.next() {
            let mut request_split = request_line.split_whitespace();

            if let Some(method_str) = request_split.next() {
                method = HttpMethod::parse(&method_str)?;
            } else {
                return Err(anyhow!("Failed to get http method, no data found."));
            }

            if let Some(path_str) = request_split.next() {
                path = path_str.into();
            } else {
                return Err(anyhow!("Failed to get path, no more data found."));
            }

            if let Some(version_str) = request_split.next() {
                http_version = version_str.into();
            } else {
                return Err(anyhow!("Failed to get version, no more data found."));
            }
        } else {
            return Err(anyhow!("Failed to parse request, no data found."));
        }

        let mut headers: HashMap<String, String> = HashMap::new();

        for header in reader_lines {
            if let Some((header_key, header_value)) = header.split_once(": ") {
                headers.insert(header_key.into(), header_value.into());
            } else {
                return Err(anyhow!(
                    "Failed to parse header, values should be separated with `: `, got: {}.",
                    header
                ));
            }
        }

        Ok(Self {
            method,
            path,
            http_version,
            headers,
            body: None,
            vars: None,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
enum HttpCode {
    Ok,
    NotFound,
    InternalServerError,
    BadRequest,
    Created,
}

impl HttpCode {
    fn to_tcp_format(&self) -> &'static str {
        match self {
            HttpCode::Ok => "200 OK",
            HttpCode::NotFound => "404 Not Found",
            HttpCode::InternalServerError => "500 Internal Error",
            HttpCode::BadRequest => "400 Bad Request",
            HttpCode::Created => "201 Created",
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

fn handle_connection(stream: TcpStream, config: &Cli) {
    let request = Request::from_stream(&stream).unwrap();

    let mut response = Response {
        http_code: HttpCode::Ok,
        headers: HashMap::new(),
        content: None,
    };

    match request.method {
        HttpMethod::Get => {
            match request.path.as_str() {
                "/" => {}
                path => {
                    if path.starts_with("/echo") {
                        let echo_word = path[1..].split_once('/').unwrap().1;

                        response
                            .headers
                            .insert("Content-Type".into(), "text/plain".into());
                        response
                            .headers
                            .insert("Content-Length".into(), format!("{}", echo_word.len()));

                        response.content = Some(echo_word.to_owned());
                    } else if path.starts_with("/user-agent") {
                        response
                            .headers
                            .insert("Content-Type".into(), "text/plain".into());
                        response.headers.insert(
                            "Content-Length".into(),
                            format!("{}", request.headers.get("User-Agent").unwrap().len()),
                        );

                        response.content =
                            Some(request.headers.get("User-Agent").unwrap().to_owned());
                    } else if path.starts_with("/files") {
                        let path_split = path[1..].split_once('/');

                        match path_split {
                            Some((_, file_name)) => {
                                match &config.directory {
                                    Some(dir) => {
                                        // Get file
                                        match fs::read_to_string(format!(
                                            "{}{}",
                                            dir.display(),
                                            file_name
                                        )) {
                                            Ok(data) => {
                                                response.headers.insert(
                                                    "Content-Type".into(),
                                                    "application/octet-stream".into(),
                                                );
                                                response.headers.insert(
                                                    "Content-Length".into(),
                                                    format!("{}", data.len()),
                                                );
                                                response.content = Some(data)
                                            }
                                            Err(_) => response.http_code = HttpCode::NotFound,
                                        };
                                    }
                                    None => {
                                        eprintln!("CRITICAL: No files directory was set!");
                                        response.http_code = HttpCode::InternalServerError;
                                    }
                                }
                            }
                            None => {
                                response.http_code = HttpCode::Ok;
                            }
                        };
                    } else {
                        response.http_code = HttpCode::NotFound
                    }
                }
            };
        }
        HttpMethod::Post => {
            if request.path.as_str().starts_with("/files") {
                let path_split = request.path[1..].split_once("/");
                if let Some(content_type) = request.headers.get(CONTENT_TYPE_HEADER) {
                    if content_type != "application/octet-stream" {
                        response.http_code = HttpCode::BadRequest;
                        let response_msg = format!(
                            "Unsupported content type `{}` expected `application/octet-stream`",
                            content_type
                        );
                        response
                            .headers
                            .insert(CONTENT_LENGTH_HEADER.into(), response_msg.len().to_string());
                        response.content = Some(response_msg.into());
                    }
                } else {
                    response.http_code = HttpCode::BadRequest;
                    let response_msg = "Expected content type header but got nothing.";
                    response
                        .headers
                        .insert(CONTENT_LENGTH_HEADER.into(), response_msg.len().to_string());
                    response.content = Some(response_msg.into());
                }

                if response.http_code != HttpCode::BadRequest {
                    if let Some(content_length) = request.headers.get(CONTENT_LENGTH_HEADER) {
                        debug_assert_eq!(
                            content_length.parse::<usize>().unwrap(),
                            request.body.to_owned().unwrap().len()
                        );
                    } else {
                        response.http_code = HttpCode::BadRequest;
                        let response_msg = format!("Missing {} header", CONTENT_LENGTH_HEADER);
                        response
                            .headers
                            .insert(CONTENT_LENGTH_HEADER.into(), response_msg.len().to_string());
                        response.content = Some(response_msg.into());
                    }
                }

                if response.http_code != HttpCode::BadRequest {
                    match path_split {
                        Some((_, file_name)) => {
                            match &config.directory {
                                Some(dir) => {
                                    // Get file
                                    let mut filepath = dir.clone();
                                    filepath.push(file_name);
                                    println!("Not mutated {:?}", dir);
                                    match File::create_new(filepath) {
                                        Ok(mut file) => {
                                            match file.write_all(
                                                request
                                                    .body
                                                    .expect("No file data to upload.")
                                                    .as_bytes(),
                                            ) {
                                                Ok(_) => {
                                                    response.http_code = HttpCode::Created;
                                                }
                                                Err(err) => {
                                                    eprintln!(
                                                        "Failed to load file to {}, got error: {}",
                                                        file_name,
                                                        err
                                                    );
                                                    response.http_code =
                                                        HttpCode::InternalServerError;
                                                }
                                            }
                                        }
                                        Err(err) => match err.kind() {
                                            std::io::ErrorKind::AlreadyExists => {
                                                response.http_code = HttpCode::BadRequest;
                                                let response_msg =
                                                    format!("File {} already exists.", file_name);
                                                response.headers.insert(
                                                    CONTENT_LENGTH_HEADER.into(),
                                                    response_msg.len().to_string(),
                                                );
                                                response.content = Some(response_msg.into());
                                            }
                                            _ => {
                                                eprintln!("CRITICAL: Could upload a user's file due to an internal server error: {}", err);
                                                response.http_code = HttpCode::InternalServerError;
                                            }
                                        },
                                    }
                                }
                                None => {
                                    eprintln!("CRITICAL: No files directory was set!");
                                    response.http_code = HttpCode::InternalServerError;
                                }
                            }
                        }
                        None => {
                            response.http_code = HttpCode::BadRequest;
                            response.content = Some("No file name sent in url, url should be formatted like /files/<file_name>".into());
                        }
                    }
                }
            } else {
                response.http_code = HttpCode::BadRequest;
            }
        }
    }
    response.write_to_stream(&stream).unwrap();
}
