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

  let mut tcp_server = TcpListener::bind(socket_addr)?;

  let mut poll = Poll::new()?;

  const SERVER: Token = Token(0);
  const CLIENT: Token = Token(1);
  poll.registry().register(&mut tcp_server, SERVER, Interest::READABLE)?;

  let mut events = Events::with_capacity(1024);
  let mut amount = 0;
  let mut client_stream: Option<TcpStream> = None;

  let mut start_timestamp: Option<u64> = None;

  let mut total_bytes: u64 = 0;

  'outer: loop {
      poll.poll(&mut events, Some(Duration::from_millis(TIMEOUT)))?;

      if events.is_empty() { 
        // timeout
        println!("Server, timeout. Quiting...");
        break;
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

                  start_timestamp = Some(get_current_usec());
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
                  // Get current time
                  let mut last_sent_time = Instant::now();

                  'blocks: while amount < cfgs.len() {
                    // Wait until it's time to send the block
                    while last_sent_time.elapsed().as_secs_f32() < cfgs[amount].send_time_gap {
                      continue;
                    }
                    last_sent_time = Instant::now();

                    unsafe {
                      // create fake dtp header
                      let hdr = format!("#@{} {} {} {} {}@#",
                        amount, 
                        get_current_usec(),
                        cfgs[amount].block_size, 
                        cfgs[amount].priority, 
                        cfgs[amount].deadline,
                      );
                      let hdr_bytes = hdr.as_bytes();
                      for i in 0..hdr.len() {
                        DATA_BUF[i] = hdr_bytes[i];
                      }
                      // start writing
                      let data_len: usize = hdr_bytes.len() + 
                        if (cfgs[amount].block_size as usize) < MAX_BLOCK_SIZE{
                          cfgs[amount].block_size as usize
                        } else {
                          MAX_BLOCK_SIZE
                        }
                      ;
                     
                      match stream.write_all(&DATA_BUF[..data_len])
                      {
                        Ok(()) => {
                          total_bytes += cfgs[amount].block_size as u64;
                          amount += 1; 
                          // println!("{}: Write {} bytes!", amount, data_len);
                        },
                        Err(err) => {
                          if err.kind() == io::ErrorKind::WouldBlock {
                            total_bytes += cfgs[amount].block_size as u64; 
                            amount += 1; //* key bug cause
                            // println!("Would Block");
                            break 'blocks;
                          } else {
                            return Err(Box::new(err));
                          }
                        }
                      }
                      if let Err(err) = stream.flush() {
                        if err.kind() == io::ErrorKind::WouldBlock {
                          break 'blocks;
                        } else {
                          return Err(Box::new(err));
                        }
                      }
                    }
                  // 'blocks: while amount < cfgs.len() {
                  //   unsafe {
                  //     // create fake dtp header
                  //     let hdr = format!("#@{} {} {} {} {}@#",
                  //       amount, 
                  //       get_current_usec(),
                  //       cfgs[amount].block_size, 
                  //       cfgs[amount].priority, 
                  //       cfgs[amount].deadline,
                  //       );
                  //     let hdr_bytes = hdr.as_bytes();
                  //     for i in 0..hdr.len() {
                  //       DATA_BUF[i] = hdr_bytes[i];
                  //     }
                  //     // start writing
                  //     let data_len: usize = hdr_bytes.len() + 
                  //       if (cfgs[amount].block_size as usize) < MAX_BLOCK_SIZE{
                  //         cfgs[amount].block_size as usize
                  //       } else {
                  //         MAX_BLOCK_SIZE
                  //       }
                  //     ;
                  //     // let mut data_sent: usize = 0;
                  //     // while data_sent < data_len {
                  //     //   match stream.write(&DATA_BUF[data_sent..data_len])
                  //     //   {
                  //     //     Ok(n) if n + data_sent < data_len => data_sent += n,
                  //     //     Ok(_) => {
                  //     //       println!("{}: Write {} bytes !", amount, data_len);
                  //     //       break;
                  //     //     },
                  //     //     Err(err) => {
                  //     //       if err.kind() == io::ErrorKind::WouldBlock {
                  //     //         // println!("Would Block!");
                  //     //         amount += 1; 
                  //     //         break 'blocks;
                  //     //       } else {
                  //     //         return Err(Box::new(err));
                  //     //       }
                  //     //     }
                  //     //   }
                  //     // }
                  //     match stream.write_all(&DATA_BUF[..data_len])
                  //     {
                  //       Ok(()) => {
                  //         amount += 1; 
                  //         // println!("{}: Write {} bytes!", amount, data_len);
                  //       },
                  //       Err(err) => {
                  //         if err.kind() == io::ErrorKind::WouldBlock {
                  //           amount += 1; //* key bug cause
                  //           // println!("Would Block");
                  //           break 'blocks;
                  //         } else {
                  //           return Err(Box::new(err));
                  //         }
                  //       }
                  //     }
                  //     if let Err(err) = stream.flush() {
                  //       if err.kind() == io::ErrorKind::WouldBlock {
                  //         break 'blocks;
                  //       } else {
                  //         return Err(Box::new(err));
                  //       }
                  //     }
                  // }
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
      if amount == cfgs.len() {
        // println!("Blocks send complete!");
        break 'outer;
      }
  }
  let end_timestamp = Some(get_current_usec());
  eprintln!("connection closed, you can see result in client.log");

  let total_time = end_timestamp.unwrap() - start_timestamp.unwrap(); 
  eprintln!("total_bytes={}, total_time(us)={}, throughput(B/s)={}", total_bytes, total_time, total_bytes / (total_time / 1000 / 1000));
  return Ok(());
}
