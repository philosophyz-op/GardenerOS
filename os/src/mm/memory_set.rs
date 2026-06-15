use core::arch::asm;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use alloc::sync::Arc;
use lazy_static::*;
use riscv::register::satp;
use xmas_elf::ElfFile;

use crate::mm::{
    PageTable, PageTableEntry, PTEFlags, VirtPageNum, VirtAddr, PhysPageNum, PhysAddr,
    FrameTracker, frame_alloc, VPNRange, StepByOne,
};
use crate::sync::UPSafeCell;
use crate::config::{
    MEMORY_END, PAGE_SIZE, TRAMPOLINE, TRAP_CONTEXT, USER_STACK_SIZE,
};

extern "C" {
    fn stext(); fn etext(); fn srodata(); fn erodata();
    fn sdata(); fn edata(); fn sbss_with_stack(); fn ebss();
    fn ekernel(); fn strampoline();
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MapType { Identical, Framed }

bitflags::bitflags! {
    pub struct MapPermission: u8 {
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
    }
}

pub struct MapArea {
    vpn_range: VPNRange,
    data_frames: BTreeMap<VirtPageNum, FrameTracker>,
    map_type: MapType,
    map_perm: MapPermission,
}

impl MapArea {
    pub fn new(start_va: VirtAddr, end_va: VirtAddr, map_type: MapType, map_perm: MapPermission) -> Self {
        let start_vpn = start_va.floor();
        let end_vpn = end_va.ceil();
        Self { vpn_range: VPNRange::new(start_vpn, end_vpn), data_frames: BTreeMap::new(), map_type, map_perm }
    }
    fn map_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        let ppn = match self.map_type {
            MapType::Identical => PhysPageNum(vpn.0),
            MapType::Framed => {
                let frame = frame_alloc().unwrap();
                let ppn = frame.ppn;
                self.data_frames.insert(vpn, frame);
                ppn
            }
        };
        let pte_flags = PTEFlags::from_bits(self.map_perm.bits).unwrap();
        page_table.map(vpn, ppn, pte_flags);
    }
    pub fn map(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range { self.map_one(page_table, vpn); }
    }
    pub fn copy_data(&mut self, page_table: &mut PageTable, data: &[u8]) {
        assert_eq!(self.map_type, MapType::Framed);
        let mut start = 0;
        let mut current_vpn = self.vpn_range.get_start();
        let len = data.len();
        loop {
            let src = &data[start..len.min(start + PAGE_SIZE)];
            let dst = &mut page_table
                .translate(current_vpn)
                .unwrap()
                .ppn()
                .get_bytes_array()[..src.len()];
            dst.copy_from_slice(src);
            start += PAGE_SIZE;
            if start >= len { break; }
            current_vpn.step();
        }
    }
}

pub struct MemorySet {
    page_table: PageTable,
    areas: Vec<MapArea>,
}

impl MemorySet {
    pub fn new_bare() -> Self {
        Self { page_table: PageTable::new(), areas: Vec::new() }
    }
    pub fn token(&self) -> usize { self.page_table.token() }
    pub fn insert_framed_area(&mut self, start_va: VirtAddr, end_va: VirtAddr, permission: MapPermission) {
        self.push(MapArea::new(start_va, end_va, MapType::Framed, permission), None);
    }
    fn push(&mut self, mut map_area: MapArea, data: Option<&[u8]>) {
        map_area.map(&mut self.page_table);
        if let Some(data) = data { map_area.copy_data(&mut self.page_table, data); }
        self.areas.push(map_area);
    }
    fn map_trampoline(&mut self) {
        self.page_table.map(
            VirtAddr::from(TRAMPOLINE).into(),
            PhysAddr::from(strampoline as *const () as usize).into(),
            PTEFlags::R | PTEFlags::X,
        );
    }
    pub fn activate(&self) {
        let satp = self.page_table.token();
        unsafe { satp::write(satp); asm!("sfence.vma"); }
    }
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.page_table.translate(vpn)
    }
    pub fn new_kernel() -> Self {
        // 调试打印：内核段地址
        println!("=== Kernel memory layout ===");
        println!("stext: {:#x}, etext: {:#x}", stext as usize, etext as usize);
        println!("srodata: {:#x}, erodata: {:#x}", srodata as usize, erodata as usize);
        println!("sdata: {:#x}, edata: {:#x}", sdata as usize, edata as usize);
        println!("sbss_with_stack: {:#x}, ebss: {:#x}", sbss_with_stack as usize, ebss as usize);
        println!("ekernel: {:#x}", ekernel as usize);
        println!("MEMORY_END: {:#x}", MEMORY_END);

        let mut memory_set = Self::new_bare();
        memory_set.map_trampoline();

        // 映射 .text
        memory_set.push(MapArea::new(
            (stext as usize).into(), (etext as usize).into(),
            MapType::Identical, MapPermission::R | MapPermission::X,
        ), None);
        // 映射 .rodata
        memory_set.push(MapArea::new(
            (srodata as usize).into(), (erodata as usize).into(),
            MapType::Identical, MapPermission::R,
        ), None);
        // 映射 .data
        memory_set.push(MapArea::new(
            (sdata as usize).into(), (edata as usize).into(),
            MapType::Identical, MapPermission::R | MapPermission::W,
        ), None);
        // 映射 .bss
        memory_set.push(MapArea::new(
            (sbss_with_stack as usize).into(), (ebss as usize).into(),
            MapType::Identical, MapPermission::R | MapPermission::W,
        ), None);

        // 物理内存映射（从 ekernel 到 MEMORY_END，页对齐）
        let start_va = VirtAddr::from(((ekernel as usize + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)));
        let end_va = VirtAddr::from((MEMORY_END) & !(PAGE_SIZE - 1));
        println!("Physical memory mapping: start_va={:?}, end_va={:?}", start_va, end_va);
        memory_set.push(MapArea::new(
            start_va,
            end_va,
            MapType::Identical,
            MapPermission::R | MapPermission::W,
        ), None);

        memory_set
    }
    pub fn from_elf(elf_data: &[u8]) -> (Self, usize, usize) {
        let mut memory_set = Self::new_bare();
        memory_set.map_trampoline();
        let elf = ElfFile::new(elf_data).unwrap();
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");
        let ph_count = elf_header.pt2.ph_count();
        let mut max_end_vpn = VirtPageNum(0);
        for i in 0..ph_count {
            let ph = elf.program_header(i).unwrap();
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_va: VirtAddr = (ph.virtual_addr() as usize).into();
                let end_va: VirtAddr = ((ph.virtual_addr() + ph.mem_size()) as usize).into();
                let mut map_perm = MapPermission::U;
                let ph_flags = ph.flags();
                if ph_flags.is_read() { map_perm |= MapPermission::R; }
                if ph_flags.is_write() { map_perm |= MapPermission::W; }
                if ph_flags.is_execute() { map_perm |= MapPermission::X; }
                let map_area = MapArea::new(start_va, end_va, MapType::Framed, map_perm);
                max_end_vpn = map_area.vpn_range.get_end();
                memory_set.push(map_area, Some(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize]));
            }
        }
        let max_end_va: VirtAddr = max_end_vpn.into();
        let mut user_stack_bottom: usize = max_end_va.into();
        user_stack_bottom += PAGE_SIZE;
        let user_stack_top = user_stack_bottom + USER_STACK_SIZE;
        memory_set.push(MapArea::new(
            user_stack_bottom.into(), user_stack_top.into(),
            MapType::Framed, MapPermission::R | MapPermission::W | MapPermission::U,
        ), None);
        memory_set.push(MapArea::new(
            TRAP_CONTEXT.into(), TRAMPOLINE.into(),
            MapType::Framed, MapPermission::R | MapPermission::W,
        ), None);
        (memory_set, user_stack_top, elf.header.pt2.entry_point() as usize)
    }
}

lazy_static! {
    pub static ref KERNEL_SPACE: Arc<UPSafeCell<MemorySet>> = Arc::new(unsafe {
        UPSafeCell::new(MemorySet::new_kernel())
    });
}

#[allow(unused)]
pub fn remap_test() {
    let kernel_space = KERNEL_SPACE.exclusive_access();
    let mid_text: VirtAddr = ((stext as *const () as usize + etext as *const () as usize) / 2).into();
    let mid_rodata: VirtAddr = ((srodata as *const () as usize + erodata as *const () as usize) / 2).into();
    let mid_data: VirtAddr = ((sdata as *const () as usize + edata as *const () as usize) / 2).into();
    assert_eq!(kernel_space.page_table.translate(mid_text.floor()).unwrap().writable(), false);
    assert_eq!(kernel_space.page_table.translate(mid_rodata.floor()).unwrap().writable(), false);
    assert_eq!(kernel_space.page_table.translate(mid_data.floor()).unwrap().executable(), false);
    println!("remap_test passed!");
}
