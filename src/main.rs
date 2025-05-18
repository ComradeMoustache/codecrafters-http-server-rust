use std::io::{Read, Write, BufRead};
use std::net::{TcpListener, TcpStream};

fn main() {
    // You can use print statements as follows for debugging, they'll be visible when running tests.
    println!("Logs from your program will appear here!");

    // Uncomment this block to pass the first stage

    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                //println!("accepted new connection");
                handle_connection(&mut stream);
                //println!("Returned 200 OK to {}", stream.peer_addr());
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}

enum RequestType {
    Get
}

struct RequestLine<'a> {
    request_type: RequestType,
    request_target: &'a str,
    http_version: &'a str,
}

fn handle_connection(stream: &mut TcpStream) {
    // Seems like the stream reader only works with bytes
    let mut buffer = [0; 512];
    //let mut buffer = String::new();
    // let mut buffer: Vec<> = ;
    println!("here");
    _ = stream.read(&mut buffer).unwrap();
    //let mut new_stream = stream.read(buffer);
    //new_stream.read_to_string(&mut buffer).unwrap();

    let request = String::from_utf8(buffer.into()).unwrap();
    println!("{:?}", request);
    println!("{:?}", request.split("\r\n").next().unwrap());
    let request_line = request.split("\r\n").next().unwrap();
    let path = request_line.split(' ').nth(1).unwrap();
    println!("{:?}", path);

    match path {
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
