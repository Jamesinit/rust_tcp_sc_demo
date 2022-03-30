#[macro_use]
extern crate log;

use std::net;
use std::net::SocketAddr;
use std::io::prelude::*;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::{io};

use ring::rand::*;

use dtp_utils::*;

use std::fs::File;
use std::path::Path;

use mio::{Token, Poll, event::*, Interest};
use mio::net::{TcpListener, TcpStream};

use nix::sys::{socket, socket::sockopt::TcpCongestion};
use std::{os::unix::io::AsRawFd, ffi::OsString};

macro_rules! log {
    ($file:expr, $display:expr, $($arg:tt)*) => {
        let s = format!($($arg)*);
        match $file.write_all(s.as_bytes()) {
            Err(why) => panic!("couldn't write to {}: {}", $display, why),
            _ => (),
        };
    }
}

const USAGE: &str = "Usage:
server [options] ADDR PORT CONFIG
server -h | --help

Options:
-h --help                Show this screen.
";


const TIMEOUT: u64 = 5000;
const MAX_BLOCK_SIZE: usize = 1000000;
static mut DATA_BUF: [u8; MAX_BLOCK_SIZE + 10000] = [0; MAX_BLOCK_SIZE + 10000];

fn main() -> Result<(), Box<dyn Error>> {
    let args = docopt::Docopt::new(USAGE)
    .and_then(|dopt| dopt.parse())
    .unwrap_or_else(|e| e.exit());
    eprintln!("Begin TCP baseline server");
    eprintln!("server start, timestamp: {}", get_current_usec());
    
    env_logger::builder()
    .format_timestamp_nanos()
    .init();

    // parse address
    let addr = args.get_str("ADDR");
    let port = args.get_str("PORT");
    let peer_addr: String = format!("{}:{}", addr, port);
    let socket_addr = peer_addr.parse::<SocketAddr>()?;
    // load dtp configs
    let config_file = args.get_str("CONFIG");
    let cfgs = get_dtp_config(config_file);
    if cfgs.len() <= 0 {
        eprintln!("Error dtp config length: 0");
        panic!("Error: No dpt config is found");
    }
    
    // println!("socket_addr: {:?}", socket_addr);
    // create TCP listener
    let mut tcp_server = TcpListener::bind(socket_addr)?;
    if let Ok(val) = socket::getsockopt::<TcpCongestion>(tcp_server.as_raw_fd(), TcpCongestion) {
        println!("{:?}", val);
    }
    
    match socket::setsockopt(tcp_server.as_raw_fd(), TcpCongestion, &OsString::from("reno")) {
        Ok(()) => println!("set cc to reno"),
        Err(e) => println!("setsockopt err {:?}", e),
    }
    
    if let Ok(val) = socket::getsockopt::<TcpCongestion>(tcp_server.as_raw_fd(), TcpCongestion) {
        println!("{:?}", val);
    }
    
    let mut poll = Poll::new()?;
    
    const SERVER: Token = Token(0);
    const CLIENT: Token = Token(1);
    poll.registry().register(&mut tcp_server, SERVER, Interest::READABLE)?;
    
    let mut events = Events::with_capacity(1024);
    let mut amount = 0;
    let mut client_stream: Option<TcpStream> = None;
    
    let mut gap_sum: Vec<u64> = vec![0; cfgs.len()]; // us
    gap_sum[0] = (cfgs[0].send_time_gap * 1_000_000.0) as u64;
    for i in 1..cfgs.len() {
        gap_sum[i] = (cfgs[i].send_time_gap * 1_000_000.0) as u64 + gap_sum[i - 1];
    }
    
    let mut send_amount = 0;
    
    let mut start_timestamp: Option<u64> = None;
    let mut should_send_time: Option<u64> = None;
    
    let mut total_bytes: u64 = 0;
    
    let mut total_size : usize= 0;
    let mut is_timeout = false;
    'outer: loop {
        let timeout = 
            if start_timestamp.is_some() && amount < cfgs.len() {
                // during sending 
                is_timeout = false;
                let c_time = get_current_usec();
                if start_timestamp.clone().unwrap() + gap_sum[amount] > c_time {
                    start_timestamp.clone().unwrap() + gap_sum[amount] - c_time
                } else {
                    0
                }
            } else {
                is_timeout = true;
                TIMEOUT * 1000
            };
        poll.poll(&mut events, Some(Duration::from_micros(timeout)))?;
        
        if events.is_empty() { 
            if is_timeout {
                // timeout
                println!("Server, timeout. Quiting...");
                break;
            } else {
                amount += 1;
                debug!("amount in queue: {}", amount);
            }
        }
        
        // handle events
        for event in events.iter() {
            match event.token() {
                // establish connection and get TcpStream
                SERVER => {
                    match tcp_server.accept() {
                        Ok((mut stream, _addr)) => {
                            // println!("Got a connection from : {}", addr);
                            if client_stream.is_none() {
                                poll.registry().register(&mut stream, CLIENT, Interest::WRITABLE)?;
                                client_stream = Some(stream);
                                
                                let cur_time = get_current_usec();
                                start_timestamp = Some(cur_time);
                                if let Some(s_time) = start_timestamp {
                                    eprintln!("new connection, timestamp: {}", s_time);
                                }
                            } else {
                                panic!("Try to re-establishing client connection in TCP !");
                            }
                            
                        },
                        Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                        Err(err) => return Err(Box::new(err))
                    }
                },
                // Write to the client
                CLIENT => {
                    if event.is_writable() {
                        match client_stream {
                            Some(ref mut stream) => { 
                                debug!("writable!");
                                'writable: loop {
                                    // find more blocks to send
                                    let c_time = get_current_usec();
                                    if amount < cfgs.len() {
                                        // check outdated blocks
                                        while gap_sum[amount] + start_timestamp.clone().unwrap() > c_time {
                                            amount += 1;
                                            if amount >= cfgs.len() {
                                                break;
                                            }
                                        }
                                    }
                                    if send_amount >= amount {
                                        // wait until send the next block
                                        if amount < cfgs.len() {
                                            while get_current_usec() < start_timestamp.clone().unwrap() + gap_sum[amount] {
                                                // wait
                                            }
                                            amount += 1;
                                        } else {
                                            break;
                                        }
                                    }

                                    // send block
                                    while send_amount < amount {
                                        // prepare data
                                        unsafe {
                                            while(gap_sum[send_amount] + start_timestamp.clone().unwrap() > get_current_usec()){}
                                            // create fake dtp header
                                            let mut hdr: [u8; 40] = [0; 40];
                                            let amount_bytes = (send_amount as u64).to_be_bytes();
                                            hdr[0..8].clone_from_slice(&amount_bytes);
                                            let timestamp = start_timestamp.clone().unwrap() + gap_sum[send_amount];
                                            let timestamp_bytes = timestamp.to_be_bytes();
                                            hdr[8..16].clone_from_slice(&timestamp_bytes);
                                            let block_size = 
                                            if (cfgs[send_amount].block_size as usize) < MAX_BLOCK_SIZE {
                                                cfgs[send_amount].block_size as usize
                                            } else {
                                                MAX_BLOCK_SIZE
                                            } as u64; 
                                            let block_size_bytes = block_size.to_be_bytes();
                                            hdr[16..24].clone_from_slice(&block_size_bytes);
                                            hdr[24..32].clone_from_slice(&(cfgs[send_amount].priority as u64).to_be_bytes());
                                            hdr[32..40].clone_from_slice(&(cfgs[send_amount].deadline as u64).to_be_bytes());
                                            let hdr_bytes = hdr;
                                            for i in 0..hdr_bytes.len() {
                                                DATA_BUF[i] = hdr_bytes[i];
                                            }
                                            // start writing
                                            let send_len: usize = hdr_bytes.len() + block_size as usize;
                                            // write block
                                            'write: loop {
                                                match stream.write(&DATA_BUF[total_size..send_len]) {
                                                    Ok(size) => {
                                                        if size == 0 {
                                                            // connection closed
                                                            break 'outer;
                                                        }
                                                        total_size += size;
                                                        if total_size == send_len {
                                                            total_size = 0;
                                                            total_bytes += cfgs[send_amount].block_size as u64;
                                                            debug!("{}: Write {} bytes!", send_amount, send_len);
                                                            send_amount += 1; 
                                                            break 'write;
                                                        }
                                                    },
                                                    Err(err) => {
                                                        if err.kind() == io::ErrorKind::WouldBlock {
                                                            debug!("{}: Would Block, sent {}, remain {}", send_amount, total_size, send_len - total_size);
                                                            break 'writable;
                                                        } else {
                                                            return Err(Box::new(err));
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            None => panic!("Event is writable, but there is no client stream!")
                        }
                    }
                    if event.is_readable() {
                        panic!("Something is readable in server, but it is abnormal...");
                    }
                },
                _ => unreachable!()
            }
        }
            
        // check if configs are fully sent to the peer
        if amount >= cfgs.len() && send_amount >= amount {
            println!("Blocks send complete!");
            break 'outer;
        }
    }
    let end_timestamp = Some(get_current_usec());
    eprintln!("connection closed, you can see result in client.log");
    
    let total_time = end_timestamp.unwrap() - start_timestamp.unwrap(); 
    let throughput: f64;
    if (total_time as f64 / 1000.0 / 1000.0 ) == 0.0 {
        throughput = 99999999999999999.0;
    } else {
        throughput = total_bytes as f64 / (total_time as f64 / 1000.0 / 1000.0);
    }
    eprintln!("total_bytes={}, total_time(us)={}, throughput(B/s)={}", total_bytes, total_time, throughput);
    return Ok(());
}
    