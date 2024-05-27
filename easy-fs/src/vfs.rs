use super::{
    block_cache_sync_all, get_block_cache, BlockDevice, DirEntry, DiskInode, DiskInodeType,
    EasyFileSystem, DIRENT_SZ,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, MutexGuard};
/// Virtual filesystem layer over easy-fs
pub struct Inode {
    block_id: usize,
    block_offset: usize,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
        }
    }
    /// Call a function over a disk inode to read it
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }
    /// Call a function over a disk inode to modify it
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }
    /// Find inode under a disk inode by name
    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        // assert it is a directory
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device,),
                DIRENT_SZ,
            );
            if dirent.name() == name {
                return Some(dirent.inode_id() as u32);
            }
        }
        None
    }
    /// Find inode under current inode by name
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                ))
            })
        })
    }
    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }
    /// Create inode under current inode by name
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();
        let op = |root_inode: &DiskInode| {
            // assert it is a directory
            assert!(root_inode.is_dir());
            // has the file been created?
            self.find_inode_id(name, root_inode)
        };
        if self.read_disk_inode(op).is_some() {
            return None;
        }
        // create a new file
        // alloc a inode with an indirect block
        let new_inode_id = fs.alloc_inode();
        // initialize inode
        let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
        get_block_cache(new_inode_block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                new_inode.initialize(DiskInodeType::File);
            });
        self.modify_disk_inode(|root_inode| {
            // append file in the dirent
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // write dirent
            let dirent = DirEntry::new(name, new_inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });

        let (block_id, block_offset) = fs.get_disk_inode_pos(new_inode_id);
        block_cache_sync_all();
        // return inode
        Some(Arc::new(Self::new(
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        )))
        // release efs lock automatically by compiler
    }
    /// List inodes under current inode
    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                v.push(String::from(dirent.name()));
            }
            v
        })
    }
    /// Read data from current inode
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }
    /// Write data to current inode
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }
    /// Clear the data in current inode
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
            for data_block in data_blocks_dealloc.into_iter() {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }
    /// Link At operation, link to the old name for the new name
    pub fn link_at(&self, old_name: &str, new_name: &str) -> isize {
        let mut fs = self.fs.lock();
        // first try to find the old name's inode-id
        match get_block_cache(self.block_id, Arc::clone(&self.block_device)).lock()
            .read(self.block_offset, |disk_inode|self.find_inode_id(old_name, disk_inode)) {
                None => {return -1;},
                Some(inode_id) => {
                    // have found the old name, can add a new name to point at it
                    get_block_cache(self.block_id, Arc::clone(&self.block_device)).lock()
                        .modify(self.block_offset, |dir_root: &mut DiskInode| {
                            // before link, there are dir_root.size as usize / DIRENT_SZ files in the root dir
                            // NOTICE! the func `increase_size`, has a para called new_size, it's meaning is, add to ..., not add it's value!
                            self.increase_size(dir_root.size as u32 + DIRENT_SZ as u32, dir_root, &mut fs);
                            dir_root.write_at(dir_root.size as usize, DirEntry::new(new_name, inode_id).as_bytes(), &self.block_device);
                            let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);

                            let new_inode = Arc::new(Self::new(block_id, block_offset, self.fs.clone(), self.block_device.clone()));
                            get_block_cache(new_inode.block_id, Arc::clone(&new_inode.block_device)).lock()
                                .modify(new_inode.block_offset, |disk_node: &mut DiskInode|disk_node.add_ref());
                                // to avoid para-same-name, use `disk_node` to be the meaning of `disk_inode`

                            block_cache_sync_all();
                            0
                    })
                }
        }
    }
    /// unlink_at, use this func to rm the link which has been established
    pub fn unlink_at(&self, name: &str) -> isize {
        let mut fs = self.fs.lock();
        match get_block_cache(self.block_id, Arc::clone(&self.block_device)).lock()
            .modify(self.block_offset, |disk_inode: &mut DiskInode| {
                let file_count = (disk_inode.size as usize) / DIRENT_SZ;
                let mut dirent = DirEntry::empty();
                for i in 0..file_count {
                    assert_eq!(
                        disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device),
                        DIRENT_SZ,
                    );
                    if dirent.name() == name {
                        let id = Some(dirent.inode_id());
                        // release this space, it will leave a hole, we can use the last inode to fill in the hole
                        if i == file_count - 1 {
                            // this is the last one, just free itOK
                            disk_inode.size -= DIRENT_SZ as u32;
                        } else {
                            // not the last one, use the last inode to instead
                            disk_inode.read_at(DIRENT_SZ * (file_count - 1), dirent.as_bytes_mut(), &self.block_device);
                            // replace the hole
                            disk_inode.write_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device);
                            disk_inode.size -= DIRENT_SZ as u32;
                        }
                        return id;
                    }
                }
                None
            }) {
            None => {return -1;},
            Some(inode_id) => {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                let new_inode = Arc::new(Self::new(block_id, block_offset, self.fs.clone(), self.block_device.clone()));
                // same reason, to avoid para-same-name, use `disk_node` to be the meaning of `disk_inode`
                get_block_cache(new_inode.block_id, Arc::clone(&new_inode.block_device)).lock()
                    .modify(new_inode.block_offset, |disk_node: &mut DiskInode| {
                        disk_node.minus_ref();
                        if disk_node.can_remove() {
                            // delete the file
                            let size = disk_node.size;
                            let data_blocks_dealloc = disk_node.clear_size(&self.block_device);
                            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
                            for data_block in data_blocks_dealloc.into_iter() {
                                fs.dealloc_data(data_block);
                            }
                        }
                    });
                block_cache_sync_all();
                0
            }
        }
    }
    /// fstat_id, use this func to get the "inode_id" of the file
    pub fn fstat_id(&self) -> usize {
        let fs = self.fs.lock();
        fs.get_inode_by_pos(self.block_id, self.block_offset)
    }
    /// fstat_nlink, use this func to get the num of established links
    pub fn fstat_nlink(&self) -> usize {
        let _fs = self.fs.lock();
        fn get_ref_cnt(node: &DiskInode) -> usize {
            node.ref_cnt
        }
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock().read(self.block_offset, |node| get_ref_cnt(node))
    }
    /// get_mode_id, use this func to get the mode of current inode id, id will be:
    /// 1  ->  dir;
    /// 2  ->  file;
    /// 0  ->  not defined;
    pub fn get_mode_id(&self) -> usize {
        let _fs = self.fs.lock();
        fn mode_map(node: &DiskInode) -> usize {
            if node.is_dir() {
                return 1;
            } else if node.is_file() {
                return 2;
            } else {
                return 0;
            }
        }
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock().read(self.block_offset, |node| mode_map(node))
    }
}
