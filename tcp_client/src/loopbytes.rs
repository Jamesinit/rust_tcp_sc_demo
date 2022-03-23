use std::cmp::min;
pub struct LoopBytes {
    pub bytes: Vec<u8>,
    pub head: usize,
    pub tail: usize,
    pub length: usize,
    pub capacity: usize
}

impl Default for LoopBytes {
    fn default() -> Self {
        return LoopBytes::new(65535);
    }
}

impl LoopBytes {
    pub fn new(capacity: usize) -> Self {
        return LoopBytes {
            bytes: vec![0; capacity],
            head: 0,
            tail: 0,
            length: 0,
            capacity: capacity
        };
    }

    pub fn size(&self) -> usize {
        return self.length;
    }

    pub fn remaining(&self) -> usize {
        return self.capacity - 1 - self.size();
    }

    // push the bytes from the buffer to the loop array
    pub fn push(&mut self, buf: &[u8], size: usize) -> usize {
        if size == 0 {
            return 0;
        }
        let remaining = self.remaining();
        if remaining > 0 {
            let push_size = min(size, remaining);
            if self.tail + push_size < self.capacity {
                self.bytes[self.tail..self.tail + push_size].clone_from_slice(&buf[..push_size]);
                self.tail += push_size;
            } else {
                assert!(self.head <= self.tail);
                let remain = push_size - (self.capacity - self.tail);
                self.bytes[self.tail..self.capacity].clone_from_slice(&buf[..push_size - remain]);
                self.bytes[..remain].clone_from_slice(&buf[push_size - remain..push_size]);
                self.tail += push_size;
                self.tail -= self.capacity;
            }
            self.length += push_size;
            return push_size;
        } else {
            return 0;
        }
    }

    // pop the first s bytes of content in the buffer
    pub fn pop(&mut self, buf: &mut[u8], s: usize) -> usize {
        if s == 0 {
            return 0;
        }
        let size = self.size();
        if size > 0 {
            let pop_size = min(s, size); 
            if self.head + pop_size < self.capacity {
                buf[..pop_size].clone_from_slice(&self.bytes[self.head..self.head + pop_size]);
                self.head += pop_size;
            } else {
                assert!(self.head >= self.tail);
                let remain = pop_size - (self.capacity - self.head);
                buf[..pop_size - remain].clone_from_slice(&self.bytes[self.head..self.capacity]);
                buf[pop_size - remain..pop_size].clone_from_slice(&self.bytes[..remain]);
                self.head += pop_size;
                self.head -= self.capacity;
            }
            self.length -= pop_size;
            return pop_size;
        } else {
            return 0;
        }
    }

    // remove the first s bytes without returning the content
    pub fn drop(&mut self, s: usize) -> usize {
        if s == 0 {
            return 0;
        }
        let size = self.size();
        if size > 0 {
            let pop_size = min(s, size); 
            if self.head + pop_size < self.capacity {
                self.head += pop_size;
            } else {
                assert!(self.head >= self.tail);
                let remain = pop_size - (self.capacity - self.head);
                self.head += pop_size;
                self.head -= self.capacity;
            }
            self.length -= pop_size;
            return pop_size;
        } else {
            return 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push() {
        let mut loopbytes = LoopBytes::new(128);
        let mut buf: [u8; 128] = [0; 128];
        assert_eq!(100, loopbytes.push(&buf, 100));
        assert_eq!(100, loopbytes.tail);
        assert_eq!(27, loopbytes.push(&buf, 100));
        assert_eq!(127, loopbytes.tail);
        loopbytes.pop(&mut buf, 100);
        assert_eq!(100, loopbytes.head);
        assert_eq!(100, loopbytes.remaining());
        assert_eq!(127, loopbytes.tail);
        assert_eq!(100, loopbytes.push(&buf, 100));
        assert_eq!(99, loopbytes.tail);
    }


    #[test]
    fn pop() {
        let mut loopbytes = LoopBytes::new(128);
        let mut buf: [u8; 128] = [0; 128];
        assert_eq!(127, loopbytes.push(&buf, 127));
        assert_eq!(127, loopbytes.pop(&mut buf, 127));
        assert_eq!(127, loopbytes.head);
        assert_eq!(127, loopbytes.remaining());
        assert_eq!(100, loopbytes.push(&buf, 100));
        assert_eq!(127, loopbytes.head);
        assert_eq!(100, loopbytes.pop(&mut buf, 100));
    }
}
