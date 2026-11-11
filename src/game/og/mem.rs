//! A flat 32-bit memory for the ported game logic.
//!
//! The port keeps the original's data layout: globals live at their
//! original addresses (the retail executable's sections are mapped at
//! their virtual addresses, so constants and initialised data come from
//! the exe itself), heap objects have the original field offsets, and
//! pointers are plain u32 addresses. A translated function therefore reads
//! `[esi+0x9a]` as `m.r32(esi + 0x9a)`.

/// Size of the address space we back (64 MiB).
const SPACE: usize = 0x0400_0000;
/// Heap objects (`operator new`) start here, well above the image.
const HEAP_BASE: u32 = 0x0100_0000;
/// Scratch stack for locals whose address is taken, growing down.
const STACK_TOP: u32 = 0x00ff_0000;
const STACK_LIMIT: u32 = 0x00f0_0000;

#[derive(Clone)]
pub struct Mem {
    data: Vec<u8>,
    heap_next: u32,
    /// Freed blocks by size, reused first-fit by exact size.
    free: std::collections::HashMap<u32, Vec<u32>>,
    sizes: std::collections::HashMap<u32, u32>,
    pub sp: u32,
}

impl Mem {
    pub fn new() -> Mem {
        Mem {
            data: vec![0u8; SPACE],
            heap_next: HEAP_BASE,
            free: Default::default(),
            sizes: Default::default(),
            sp: STACK_TOP,
        }
    }

    /// Map a PE image's sections at their virtual addresses.
    pub fn map_pe(&mut self, exe: &[u8]) -> Result<(), String> {
        let rd16 = |o: usize| u16::from_le_bytes([exe[o], exe[o + 1]]) as usize;
        let rd32 = |o: usize| u32::from_le_bytes(exe[o..o + 4].try_into().unwrap()) as usize;
        if exe.len() < 0x40 || &exe[0..2] != b"MZ" {
            return Err("not a PE file".into());
        }
        let pe = rd32(0x3c);
        let nsec = rd16(pe + 6);
        let opt = rd16(pe + 20);
        let base = rd32(pe + 24 + 28);
        for i in 0..nsec {
            let o = pe + 24 + opt + 40 * i;
            let vsize = rd32(o + 8);
            let va = rd32(o + 12);
            let rsize = rd32(o + 16);
            let rptr = rd32(o + 20);
            let dst = base + va;
            let n = rsize.min(vsize.max(rsize));
            if dst + vsize.max(rsize) > SPACE || rptr + rsize > exe.len() {
                return Err(format!("section {i} out of range"));
            }
            self.data[dst..dst + n].copy_from_slice(&exe[rptr..rptr + n]);
        }
        Ok(())
    }

    pub fn r8(&self, a: u32) -> u8 {
        self.data[a as usize]
    }
    pub fn r16(&self, a: u32) -> u16 {
        let a = a as usize;
        u16::from_le_bytes([self.data[a], self.data[a + 1]])
    }
    pub fn r32(&self, a: u32) -> u32 {
        let a = a as usize;
        u32::from_le_bytes(self.data[a..a + 4].try_into().unwrap())
    }
    pub fn ri32(&self, a: u32) -> i32 {
        self.r32(a) as i32
    }
    pub fn rf32(&self, a: u32) -> f32 {
        f32::from_bits(self.r32(a))
    }
    pub fn rf64(&self, a: u32) -> f64 {
        let a = a as usize;
        f64::from_le_bytes(self.data[a..a + 8].try_into().unwrap())
    }
    pub fn rb(&self, a: u32) -> bool {
        self.r8(a) != 0
    }
    pub fn w8(&mut self, a: u32, v: u8) {
        self.data[a as usize] = v;
    }
    pub fn w16(&mut self, a: u32, v: u16) {
        let a = a as usize;
        self.data[a..a + 2].copy_from_slice(&v.to_le_bytes());
    }
    pub fn w32(&mut self, a: u32, v: u32) {
        let a = a as usize;
        self.data[a..a + 4].copy_from_slice(&v.to_le_bytes());
    }
    pub fn wi32(&mut self, a: u32, v: i32) {
        self.w32(a, v as u32)
    }
    pub fn wf32(&mut self, a: u32, v: f32) {
        self.w32(a, v.to_bits())
    }
    pub fn wf64(&mut self, a: u32, v: f64) {
        let a = a as usize;
        self.data[a..a + 8].copy_from_slice(&v.to_le_bytes());
    }

    pub fn bytes(&self, a: u32, n: u32) -> &[u8] {
        &self.data[a as usize..(a + n) as usize]
    }
    pub fn bytes_mut(&mut self, a: u32, n: u32) -> &mut [u8] {
        &mut self.data[a as usize..(a + n) as usize]
    }
    pub fn write(&mut self, a: u32, src: &[u8]) {
        self.data[a as usize..a as usize + src.len()].copy_from_slice(src);
    }
    /// `U_Mem_Copy` / memcpy.
    pub fn copy(&mut self, dst: u32, src: u32, n: u32) {
        self.data.copy_within(src as usize..(src + n) as usize, dst as usize);
    }
    /// `U_Mem_Clear` / memset 0.
    pub fn clear(&mut self, a: u32, n: u32) {
        self.data[a as usize..(a + n) as usize].fill(0);
    }

    /// NUL-terminated string at `a` (without the NUL).
    pub fn cstr(&self, a: u32) -> &[u8] {
        let s = &self.data[a as usize..];
        &s[..s.iter().position(|&b| b == 0).unwrap_or(0)]
    }
    pub fn strcmp(&self, a: u32, b: &[u8]) -> i32 {
        let s = self.cstr(a);
        match s.cmp(b) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        }
    }
    pub fn strcmp_mem(&self, a: u32, b: u32) -> i32 {
        let b = self.cstr(b).to_vec();
        self.strcmp(a, &b)
    }
    /// strcpy of a Rust byte string (adds the NUL).
    pub fn strcpy(&mut self, dst: u32, s: &[u8]) {
        self.write(dst, s);
        self.w8(dst + s.len() as u32, 0);
    }

    /// Bytes handed out from the top of the heap so far (freed blocks are
    /// reused, so this only grows when the free lists run dry).
    pub fn heap_used(&self) -> u32 {
        self.heap_next - HEAP_BASE
    }

    /// `operator new` / malloc: zero-filled, 4-aligned.
    pub fn alloc(&mut self, size: u32) -> u32 {
        let size = (size.max(1) + 3) & !3;
        if let Some(a) = self.free.get_mut(&size).and_then(|v| v.pop()) {
            self.clear(a, size);
            return a;
        }
        let a = self.heap_next;
        self.heap_next += size + 8;
        assert!((self.heap_next as usize) < SPACE, "og heap exhausted");
        self.sizes.insert(a, size);
        a
    }
    /// `operator delete` / free.
    pub fn free(&mut self, a: u32) {
        if a == 0 {
            return;
        }
        if let Some(&size) = self.sizes.get(&a) {
            self.free.entry(size).or_default().push(a);
        }
    }

    /// Reserve `n` bytes of scratch stack for a local whose address is
    /// passed on; release with `pop_to`.
    pub fn alloca(&mut self, n: u32) -> u32 {
        self.sp -= (n + 3) & !3;
        assert!(self.sp >= STACK_LIMIT, "og stack overflow");
        self.clear(self.sp, n);
        self.sp
    }
    pub fn pop_to(&mut self, sp: u32) {
        self.sp = sp;
    }
}

impl Default for Mem {
    fn default() -> Self {
        Self::new()
    }
}

/// `U_LinkedList` (0x4018e0..0x401cd0): the list object is
/// {numLinks, head, tail}; each link is {prev, next, object}; a
/// `U_LinkRef` is {link, object}.
pub mod list {
    use super::Mem;

    pub const SIZE: u32 = 12;

    pub fn new(m: &mut Mem) -> u32 {
        m.alloc(SIZE)
    }
    pub fn num(m: &Mem, l: u32) -> i32 {
        m.ri32(l)
    }
    fn head(m: &Mem, l: u32) -> u32 {
        m.r32(l + 4)
    }
    fn tail(m: &Mem, l: u32) -> u32 {
        m.r32(l + 8)
    }

    /// `CreateAndAddLink` (0x4019c0): append.
    pub fn add(m: &mut Mem, l: u32, obj: u32) {
        let link = m.alloc(12);
        m.w32(link + 8, obj);
        if head(m, l) == 0 {
            m.w32(l + 4, link);
        }
        let t = tail(m, l);
        if t != 0 {
            m.w32(t + 4, link);
        }
        m.w32(link, t);
        m.w32(link + 4, 0);
        m.w32(l + 8, link);
        m.wi32(l, num(m, l) + 1);
    }

    /// `CreateAndAddLinkAtStart` (0x401920).
    pub fn add_at_start(m: &mut Mem, l: u32, obj: u32) {
        let link = m.alloc(12);
        m.w32(link + 8, obj);
        if head(m, l) == 0 {
            m.w32(l + 4, link);
            m.w32(link, tail(m, l));
            m.w32(link + 4, 0);
            m.w32(l + 8, link);
        } else {
            let h = head(m, l);
            m.w32(link, 0);
            m.w32(link + 4, h);
            m.w32(l + 4, link);
            m.w32(h, link);
        }
        m.wi32(l, num(m, l) + 1);
    }

    /// `FindLink` (0x401cd0).
    pub fn find_link(m: &Mem, l: u32, obj: u32) -> u32 {
        let mut link = head(m, l);
        while link != 0 {
            if m.r32(link + 8) == obj {
                return link;
            }
            link = m.r32(link + 4);
        }
        0
    }

    /// `DeleteLink(link)` (0x401af0).
    pub fn delete_link(m: &mut Mem, l: u32, link: u32) {
        if link == 0 {
            return;
        }
        unlink(m, l, link);
    }

    fn unlink(m: &mut Mem, l: u32, link: u32) -> u32 {
        let (prev, next) = (m.r32(link), m.r32(link + 4));
        let mut kept_prev = 0;
        if link == head(m, l) {
            m.w32(l + 4, next);
        } else {
            kept_prev = prev;
            m.w32(prev + 4, next);
        }
        if link == tail(m, l) {
            m.w32(l + 8, prev);
        } else {
            m.w32(next, prev);
        }
        m.free(link);
        m.wi32(l, num(m, l) - 1);
        kept_prev
    }

    /// `DeleteLink(link, ref)` (0x401b60): the ref steps back to the
    /// previous link (or the start) so the next `next()` continues.
    pub fn delete_link_ref(m: &mut Mem, l: u32, link: u32, r: u32) {
        if link == 0 {
            return;
        }
        let prev = unlink(m, l, link);
        m.w32(r, prev);
        m.w32(r + 4, 0);
    }

    /// `FindAndDeleteLink` (0x401a50).
    pub fn find_and_delete(m: &mut Mem, l: u32, obj: u32) -> bool {
        let link = find_link(m, l, obj);
        if link == 0 {
            return false;
        }
        delete_link(m, l, link);
        true
    }

    /// `DeleteAllLinks` (0x401aa0).
    pub fn delete_all(m: &mut Mem, l: u32) {
        let mut link = head(m, l);
        while link != 0 {
            let next = m.r32(link + 4);
            m.free(link);
            link = next;
        }
        m.w32(l + 4, 0);
        m.w32(l + 8, 0);
        m.wi32(l, 0);
    }

    /// `GetNthLinkObject` (0x401c00).
    pub fn nth(m: &Mem, l: u32, n: i32) -> u32 {
        if n < 0 || n >= num(m, l) {
            return 0;
        }
        let mut link = head(m, l);
        for _ in 0..n {
            link = m.r32(link + 4);
        }
        m.r32(link + 8)
    }

    /// `DeleteFirstLink` (0x401c30).
    pub fn delete_first(m: &mut Mem, l: u32) -> u32 {
        if num(m, l) <= 0 {
            return 0;
        }
        let obj = m.r32(head(m, l) + 8);
        if obj == 0 {
            return 0;
        }
        let link = find_link(m, l, obj);
        delete_link(m, l, link);
        obj
    }

    /// `GetNextLinkObject` (0x401cb0) on a ref at `r` ({link, object}).
    pub fn next(m: &mut Mem, l: u32, r: u32) {
        let cur = m.r32(r);
        let next = if cur == 0 { head(m, l) } else { m.r32(cur + 4) };
        m.w32(r, next);
        let obj = m.r32(next + 8);
        m.w32(r + 4, obj);
    }

    /// All objects, head to tail (for code that only reads the list).
    pub fn objects(m: &Mem, l: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut link = head(m, l);
        while link != 0 {
            out.push(m.r32(link + 8));
            link = m.r32(link + 4);
        }
        out
    }
}
