#[macro_use]
extern crate log;
use std::net::ToSocketAddrs;

use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::error::Error;

use mio::net::TcpStream;

use dtp_utils::get_current_usec;
use streamparser::StreamParser;

const TIMEOUT: u64 = 5000;

const USAGE: &str = "Usage:
    client [options] ADDR PORT
    client -h | --help

    Options:
    --wire-version VERSION   The version number to send to the server [default: babababa].
    --dump-packets PATH      Dump the incoming packets as files in the given directory.
    --no-verify              Don't verify server's certificate.
    --cc-algorithm NAME      Set client congestion control algorithm [default: reno].
    -h --help                Show this screen.
";

const CLIENT: mio::Token = mio::Token(1);

#[derive(Clone, Debug, Default, Copy)]
#[repr(C)]
struct BlockInfo {
    start_timestamp: u64,
    end_timestamp: u64,
    bct: u64,
    deadline: i32,
    priority: i32,
    block_size: i32,
    id: u64
}

fn main () -> Result<(), Box<dyn Error>>{
    
    let path = Path::new("client.log");
    let log_path = Path::new("client.csv");
    let display = path.display();
    // Open a file in write-only mode, returns `io::Result<File>`
    let mut file = match File::create(&path) {
        Err(why) => panic!("couldn't create {}: {}", display, why),
        Ok(file) => file,
    };

    let mut log_file = match File::create(&log_path) {
        Err(why) => panic!("couldn't create {}: {}", log_path.display(), why),
        Ok(file) => file,
    };
    
    let mut buf = [0; 65535];
    
    env_logger::builder()
    .format_timestamp_nanos()
    .init();
    
    let args = docopt::Docopt::new(USAGE)
    .and_then(|dopt| dopt.parse())
    .unwrap_or_else(|e| e.exit());
    
    let dump_path = if args.get_str("--dump-packets") != "" {
        Some(args.get_str("--dump-packets"))
    } else {
        None
    };
    
    let url_string = format!("http://{0}:{1}", args.get_str("ADDR"), args.get_str("PORT"));
    let url = url::Url::parse(&url_string).unwrap();
    
    // Resolve server address.
    let peer_addr = url.to_socket_addrs().unwrap().next().unwrap();
    
    println!("peer_addr = {}", peer_addr);
    let s = format!("peer_addr = {}\n", peer_addr);
    match file.write_all(s.as_bytes()) {
        Err(why) => panic!("couldn't write to {}: {}", display, why),
        _ => (),
    }
    
    
    // Create a TCP socket and register it with the event loop.
    let mut client_stream = TcpStream::connect(peer_addr)?;
    println!("Connected to the server!");
    println!("local_addr: {:?}", client_stream.local_addr()?);
    // Setup the event loop.
    let mut poll = mio::Poll::new()?;
    let mut events = mio::Events::with_capacity(1024);
    
    poll.registry().register(&mut client_stream, mio::Token(1), mio::Interest::READABLE)?;
    
    let mut pkt_count = 0;
    let s =
        format!("test begin!\n\nBlockID\tbct\tBlockSize\tPriority\tDeadline\n");
    match file.write_all(s.as_bytes()) {
        Err(why) => panic!("couldn't write to {}: {}", display, why),
        _ => (),
    }
    let s =
        format!("block_id,bct,size,priority,deadline,duration\n");
    match log_file.write_all(s.as_bytes()) {
        Err(why) => panic!("couldn't write to {}: {}", display, why),
        _ => (),
    }
    let start_timestamp = std::time::Instant::now();
    let mut total_bytes: u64 = 0;
    let mut block_vec: Vec<BlockInfo> = Vec::new();
    let mut parser = StreamParser::new(65535);
    'outer: loop {
        poll.poll(&mut events, Some(std::time::Duration::from_millis(TIMEOUT)))?;
        
        if events.is_empty() {
            // TIMEOUT
            println!("Client TIMEOUT. Quiting...");
            let mut good_bytes: u64 = 0;
            for block in block_vec.iter() {
                if block.bct < block.deadline as u64 {
                    good_bytes += block.block_size as u64;
                }
            }

            let mut complete_bytes: u64 = 0;
            for block in block_vec.iter() {
                complete_bytes += block.block_size as u64;
            }

            let s =
                format!("connection closed, recv=-1 sent=-1 lost=-1 rtt=-1 cwnd=-1, total_bytes={}, complete_bytes={}, good_bytes={}, total_time={}\n", 
                    total_bytes, 
                    complete_bytes,
                    good_bytes,
                    start_timestamp.elapsed().as_micros()
                );
                match file.write_all(s.as_bytes()) {
                    Err(why) => panic!("couldn't write to {}: {}", display, why),
                    _ => (),
                }
            break;
        }
    
        for event in events.iter() {
            match event.token() {
                CLIENT => {
                    if event.is_readable() {
                        let mut connection_closed = false;
                        'recv: loop {
                            let len = match client_stream.read(&mut buf) {
                                Ok(0) => {
                                    // Reading 0 bytes means the server side has closed the stream
                                    // or the writing is done
                                    connection_closed = true;
                                    break;
                                }
                                Ok(v) => v,
                                Err(e) => {
                                    if e.kind() == std::io::ErrorKind::WouldBlock {
                                        debug!("recv() would block");
                                        break 'recv;
                                    }
                                    panic!("recv() failed: {:?}", e);
                                },
                            };
                            
                            debug!("got {} bytes", len);
                            total_bytes += len as u64;
                            if len != 0 {
                                if let Some(target_path) = dump_path {
                                    let path = format!("{}/{}.pkt", target_path, pkt_count);
                                    pkt_count += 1;
                                    
                                    if let Ok(f) = std::fs::File::create(&path) {
                                        let mut f = std::io::BufWriter::new(f);
                                        f.write_all(&buf[..len]).ok();
                                    }
                                }
                                
                                let mut total_size = 0;
                                let mut blocks: Vec<BlockInfo> = Vec::new();
                                
                                while total_size < len {
                                    total_size += parser.recv(&buf[total_size..len], len - total_size);
                                    blocks.append(&mut parser.consume());
                                }
                                
                                for block in blocks.iter() {
                                    // Log into client.log
                                    // BlockID bct BlockSize Priority Deadline
                                    let s = format!("{:<10}\t{:10}\t{:10}\t{:10}\t{:10}\n", 
                                        block.id, 
                                        block.bct, 
                                        block.block_size, 
                                        block.priority, 
                                        block.deadline
                                    );
                                    match file.write_all(s.as_bytes()) {
                                        Err(why) =>
                                        panic!("couldn't write to {}: {}", display, why),
                                        _ => (),
                                    }

                                    let s = format!("{},{},{},{},{},{}\n", 
                                        block.id, 
                                        block.bct, 
                                        block.block_size, 
                                        block.priority, 
                                        block.deadline,
                                        start_timestamp.elapsed().as_micros()
                                    );
                                    match log_file.write_all(s.as_bytes()) {
                                        Err(why) =>
                                        panic!("couldn't write to {}: {}", log_path.display(), why),
                                        _ => (),
                                    }
                                }
                                block_vec.append(&mut blocks);
                            } 
                        }
                        if connection_closed {
                            let mut good_bytes: u64 = 0;
                            for block in block_vec.iter() {
                                if block.bct < block.deadline as u64 {
                                    good_bytes += block.block_size as u64;
                                }
                            }

                            let mut complete_bytes: u64 = 0;
                            for block in block_vec.iter() {
                                complete_bytes += block.block_size as u64;
                            }

                            let s =
                                format!("connection closed, recv=-1 sent=-1 lost=-1 rtt=-1 cwnd=-1, total_bytes={}, complete_bytes={}, good_bytes={}, total_time={}\n", 
                                    total_bytes, 
                                    complete_bytes,
                                    good_bytes,
                                    start_timestamp.elapsed().as_micros()
                                );
                            match file.write_all(s.as_bytes()) {
                                Err(why) => panic!("couldn't write to {}: {}", display, why),
                                _ => (),
                            }
                            break 'outer;
                        }
                    }
                    if event.is_writable() {
                        panic!("writeable event");
                    }
                },
                _ => unreachable!()
            }
        }
    }
    Ok(())
}

mod loopbytes;
mod streamparser;