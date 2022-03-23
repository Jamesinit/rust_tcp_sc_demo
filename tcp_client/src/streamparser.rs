use crate::loopbytes::LoopBytes;
use crate::BlockInfo;
use crate::get_current_usec;

pub struct StreamParser {
    target: usize,
    has_hdr: bool,
    cur_block: BlockInfo,
    bytes: LoopBytes,
}

impl Default for StreamParser {
    fn default() -> StreamParser {
        return StreamParser::new(65535);
    }
}

impl StreamParser {
    pub fn new(size: usize) -> Self {
        return StreamParser {
            target: 40,
            has_hdr: false,
            cur_block: BlockInfo::default(),
            bytes: LoopBytes::new(size + 1),
        };
    }

    pub fn recv(&mut self, buf: &[u8], size: usize) -> usize {
        return self.bytes.push(buf, size);
    }

    pub fn consume(&mut self) -> Vec<BlockInfo> {
        let mut ret: Vec<BlockInfo> = vec![];
        let mut cost = 0;
        loop {
            debug!("size: {}", self.bytes.size());
            if self.bytes.size() >= self.target {
                cost = self.target;
                if !self.has_hdr {
                    let mut hdr: [u8; 50] = [0; 50];
                    let mut bytes: [u8; 8] = [0; 8];
                    // self.parse_hdr();
                    assert_eq!(cost, 40);
                    self.bytes.pop(&mut hdr, cost);
                    // id
                    bytes.clone_from_slice(&hdr[0..8]);
                    self.cur_block.id= u64::from_be_bytes(bytes);
                    assert!(self.cur_block.id >= 0);
                    // start timestamp
                    bytes.clone_from_slice(&hdr[8..16]);
                    self.cur_block.start_timestamp = u64::from_be_bytes(bytes);
                    // block size
                    bytes.clone_from_slice(&hdr[16..24]);
                    self.cur_block.block_size = u64::from_be_bytes(bytes) as i32;
                    assert!(self.cur_block.block_size != 0);
                    // priority
                    bytes.clone_from_slice(&hdr[24..32]);
                    self.cur_block.priority = u64::from_be_bytes(bytes) as i32;
                    assert!(self.cur_block.priority >= 0);
                    // deadline
                    bytes.clone_from_slice(&hdr[32..40]);
                    self.cur_block.deadline = u64::from_be_bytes(bytes) as i32;
                    assert!(self.cur_block.deadline >= 0);

                    debug!("parse block: {:?}", self.cur_block);

                    self.target = self.cur_block.block_size as usize;
                } else {
                    // self.record_block();
                    assert_eq!(cost, self.bytes.drop(cost));
                    self.cur_block.end_timestamp = get_current_usec();
                    self.cur_block.bct = (self.cur_block.end_timestamp - self.cur_block.start_timestamp) / 1000;
                    debug!("final block: {:?}", self.cur_block);
                    ret.push(self.cur_block);
                    self.target = 40;
                    self.cur_block = BlockInfo::default();
                }
                self.has_hdr = !self.has_hdr;
            } else {
                if !self.has_hdr {
                    cost = 0;
                } else {
                    cost = self.bytes.size();
                }
                assert_eq!(cost, self.bytes.drop(cost));
                self.target -= cost;
                debug!("remove {}, target: {}", cost, self.target);
                break;
            }
        }
        return ret;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recv() {
        let mut parser = StreamParser::new(5);
        let buf: [u8; 3] = [0, 1, 2];
        let mut pop_buf: [u8; 5] = [0; 5];
        assert_eq!(3, parser.recv(&buf, 3));
        assert_eq!(2, parser.recv(&buf, 3));
    }
}
