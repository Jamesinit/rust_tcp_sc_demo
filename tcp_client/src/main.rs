#[macro_use]
extern crate log;
use std::net::ToSocketAddrs;

use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::error::Error;

use mio::net::TcpStream;

use dtp_utils::get_current_usec;

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

#[macro_use] 
extern crate lazy_static;
extern crate regex;
use regex::Regex;
/// parse fake dtp headers from the buffer and recognize them as dtp blocks
fn parse_dtp_header (buf: &[u8], file: &mut File, display: &std::path::Display, blocks: &mut Vec<BlockInfo>) -> Result<usize, Box<dyn Error>>{
  lazy_static! {
    static ref FAKE_DTP_HDR: Regex = Regex::new(r"#@(\d+) (\d+) (\d+) (\d+) (\d+)@#").unwrap();
  }
  let mut num = 0;
  for cap in FAKE_DTP_HDR.captures_iter(std::str::from_utf8(buf)?) {
    // Each dtp header means a block is arrived or is about to arrive
    // We need to save the information of the block until the header of next sequence number is arrived.
    let new_block = BlockInfo {
      start_timestamp: cap[2].parse::<u64>()?,
      end_timestamp: 0,
      bct: std::u64::MAX,
      deadline: cap[5].parse::<i32>()?,
      priority: cap[4].parse::<i32>()?,
      block_size: cap[3].parse::<i32>()?,
      id: cap[1].parse::<u64>()?
    };

    if blocks.len() > 0 {
      // The last block is complete
      if let Some(last_block) = blocks.last_mut() {
        last_block.end_timestamp = get_current_usec();
        last_block.bct = (last_block.end_timestamp - last_block.start_timestamp) / 1000;
  
        // Log into client.log
        // BlockID bct BlockSize Priority Deadline
        let s = format!("{:<10}{:10}{:10}{:10}{:10}\n", 
          last_block.id, 
          last_block.bct, 
          last_block.block_size, 
          last_block.priority, 
          last_block.deadline
        );
        match file.write_all(s.as_bytes()) {
          Err(why) =>
            panic!("couldn't write to {}: {}", display, why),
          _ => (),
        }
      }
    }
    blocks.push(new_block);
    num += 1;
  }
  Ok(num)
}
fn main () -> Result<(), Box<dyn Error>>{
  
  let path = Path::new("client.log");
  let display = path.display();
  // Open a file in write-only mode, returns `io::Result<File>`
  let mut file = match File::create(&path) {
    Err(why) => panic!("couldn't create {}: {}", display, why),
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
    format!("test begin!\n\nBlockID  bct  BlockSize  Priority  Deadline\n");
  match file.write_all(s.as_bytes()) {
    Err(why) => panic!("couldn't write to {}: {}", display, why),
    _ => (),
  }
  let start_timestamp = std::time::Instant::now();
  let mut total_bytes: u64 = 0;
  let mut block_vec: Vec<BlockInfo> = Vec::new();
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
      let s =
                format!("connection closed, recv=-1 sent=-1 lost=-1 rtt=-1 cwnd=-1, total_bytes={}, complete_bytes={}, good_bytes={}, total_time={}\n", 
                  total_bytes, 
                  total_bytes,
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
                  
                  if let Ok(f) = std::fs::File::create(&path) {
                    let mut f = std::io::BufWriter::new(f);
                    f.write_all(&buf[..len]).ok();
                  }
                }
                
                if let Ok(blocks) = parse_dtp_header(&buf[..len], &mut file, &display, &mut block_vec) {
                  if blocks > 0 {
                    pkt_count += blocks;
                    // println!("block count: {}", pkt_count);
                  }
                }
              } 
            }
            if connection_closed {
              let mut good_bytes: u64 = 0;
              for block in block_vec.iter() {
                if block.bct < block.deadline as u64 {
                  good_bytes += block.block_size as u64;
                }
              }
              let s =
                format!("connection closed, recv=-1 sent=-1 lost=-1 rtt=-1 cwnd=-1, total_bytes={}, complete_bytes={}, good_bytes={}, total_time={}\n", 
                  total_bytes, 
                  total_bytes,
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